use super::{Packet, PacketError, PacketHead, PacketType};
use log::{debug, error, trace};
use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;

type ClientEventSender = Sender<ClientEvent>;
type ClientEventReceiver = Receiver<ClientEvent>;

struct ClientInner {
    addr: SocketAddr,
    handler: Arc<dyn ClientHandler>,
    is_auto_reconnect: AtomicBool,
    is_exit_loop: AtomicBool,
}

impl ClientInner {
    fn is_auto_reconnect(&self) -> bool {
        self.is_auto_reconnect.load(Ordering::SeqCst)
    }

    fn set_auto_reconnect(&self, yes: bool) {
        self.is_auto_reconnect.store(yes, Ordering::SeqCst);
    }

    fn is_exit_loop(&self) -> bool {
        self.is_exit_loop.load(Ordering::SeqCst)
    }

    fn set_exit_loop(&self, yes: bool) {
        self.is_exit_loop.store(yes, Ordering::SeqCst);
    }
}

pub struct Client {
    inner: Arc<ClientInner>,
    th: Option<JoinHandle<()>>,
    tx: Option<ClientEventSender>,
    is_started: bool,
}

impl Client {
    pub fn new<A: ToSocketAddrs>(addr: A, handler: Arc<impl ClientHandler + 'static>) -> Self {
        Self {
            inner: Arc::new(ClientInner {
                addr: addr
                    .to_socket_addrs()
                    .map(|mut x| x.next().unwrap())
                    .unwrap(),
                handler,
                is_auto_reconnect: AtomicBool::new(true),
                is_exit_loop: AtomicBool::new(false),
            }),
            th: None,
            tx: None,
            is_started: false,
        }
    }

    async fn read_data_frame(
        reader: &mut OwnedReadHalf,
        head: PacketHead,
    ) -> Result<Packet, PacketError> {
        trace!("接收 Data 型数据，长度 {} 字节", head.length);
        let mut data = Packet::alloc_data(head);
        let _r = reader.read_exact(data.as_mut()).await;
        Ok(data)
    }

    async fn read_packed_values_frame(
        reader: &mut OwnedReadHalf,
        head: PacketHead,
    ) -> Result<Packet, PacketError> {
        trace!("接收 PackedValues 型数据，长度 {} 字节", head.length);
        let mut data = Packet::alloc_data(head);
        let _r = reader.read_exact(data.as_mut()).await;
        Ok(data)
    }

    async fn read_loop(inner: Arc<ClientInner>, mut reader: OwnedReadHalf) {
        let mut head = [0u8; 24];
        let mut ticker = tokio::time::interval(Duration::from_millis(100));
        'outer: loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if inner.is_exit_loop() {
                        debug!("检测到退出标志, 终止接收");
                        break;
                    }
                }
                r = reader.read_exact(&mut head) => {
                    match r {
                        Ok(len) => {
                            if len != 24 {
                                error!("收到数据头不完整: {:?}", &head[..len]);
                                break 'outer;
                            }
                            if let Ok(head) = PacketHead::parse(&head) {
                                trace!("收到数据头: {:?}", head);
                                let packet = match head.type_ {
                                    PacketType::Data => Self::read_data_frame(&mut reader, head).await,
                                    PacketType::PackedValues => {
                                        Self::read_packed_values_frame(&mut reader, head).await
                                    }
                                    _ => Err(PacketError::UnknownType(head.type_ as u8)),
                                };
                                match packet {
                                    Ok(packet) => {
                                        inner.handler.on_packet(Arc::new(packet));
                                    }
                                    Err(err) => {
                                        error!("收取数据时发生异常: {}", err);
                                        break 'outer;
                                    }
                                }
                            } else {
                                error!("解析帧头时发生异常 {}", "");
                                break 'outer;
                            }
                        }
                        Err(err) => {
                            error!("读取帧头时发生异常 {}", err);
                            break 'outer;
                        }
                    }
                }
            }
        }
        inner.set_exit_loop(true);
        debug!("已经退出数据接收循环");
    }

    async fn write_loop(
        inner: Arc<ClientInner>,
        mut rx: Receiver<ClientEvent>,
        mut writer: OwnedWriteHalf,
    ) -> ClientEventReceiver {
        use tokio::time::timeout;

        let tmo = Duration::from_millis(40);
        'outer: loop {
            if inner.is_exit_loop() {
                debug!("检测到退出标志, 终止发送");
                break;
            }
            match timeout(tmo, rx.recv()).await {
                Ok(Some(ev)) => match ev {
                    ClientEvent::Idle => {}
                    ClientEvent::Packet(packet) => {
                        let head_bytes = packet.head.to_bytes();
                        if let Err(err) = writer.write_all(&head_bytes).await {
                            error!("发送帧头时发生异常 {}", err);
                            break 'outer;
                        }
                        let body_bytes = packet.data.as_ref();
                        if let Err(err) = writer.write_all(body_bytes).await {
                            error!("发送内容时发生异常 {}", err);
                            break 'outer;
                        }
                    }
                    ClientEvent::Shutdown => {
                        break 'outer;
                    }
                },
                Ok(None) => {
                    error!("数据输出队列已经关闭");
                    break 'outer;
                }
                Err(_) => {
                    // trace!("读取数据输出队列时超时");
                }
            }
        }
        inner.set_exit_loop(true);
        debug!("已经退出数据发送循环");
        rx
    }

    async fn mantain_loop(inner: Arc<ClientInner>, rx: Receiver<ClientEvent>) {
        let mut rx: Option<ClientEventReceiver> = Some(rx);
        'outer: loop {
            let r =
                tokio::time::timeout(Duration::from_millis(100), TcpStream::connect(&inner.addr))
                    .await;
            match r {
                Ok(Ok(socket)) => {
                    debug!("已成功连接到: {}", inner.addr);
                    inner.handler.on_state(ClientState::Connected);
                    inner.set_exit_loop(false);
                    // 优化小包传输
                    socket.set_nodelay(true).unwrap();
                    //
                    let (reader, writer) = socket.into_split();
                    let t1 = tokio::task::spawn(Self::read_loop(Arc::clone(&inner), reader));
                    let t2 = tokio::task::spawn(Self::write_loop(
                        Arc::clone(&inner),
                        rx.take().unwrap(),
                        writer,
                    ));
                    let (_, r) = tokio::join!(t1, t2);
                    rx = Some(r.unwrap());
                    debug!("数据收发已经全部退出");
                    inner.handler.on_state(ClientState::Disconnected);
                }
                Ok(Err(err)) => {
                    debug!("尝试连接到 {} 时发生异常: {}", inner.addr, err);
                    if !inner.is_auto_reconnect() {
                        inner.handler.on_state(ClientState::ConnectError);
                        break 'outer;
                    }
                    inner.handler.on_state(ClientState::TryReconnect);
                }
                Err(err) => {
                    debug!("尝试连接到 {} 时发生超时: {}", inner.addr, err);
                    if !inner.is_auto_reconnect() {
                        inner.handler.on_state(ClientState::ConnectTimeout);
                        break 'outer;
                    }
                    inner.handler.on_state(ClientState::TryReconnect);
                }
            }
            if inner.is_auto_reconnect() {
                let _r = tokio::time::sleep(Duration::from_millis(1000)).await;
            } else {
                break;
            }
        }
    }

    pub async fn start(&mut self) -> Result<(), Error> {
        if !self.is_started {
            let (tx, rx) = channel(100);
            let cloned_inner = Arc::clone(&self.inner);
            let th = tokio::task::spawn(async move {
                let _r = Self::mantain_loop(cloned_inner, rx).await;
            });
            self.th = Some(th);
            self.tx = Some(tx);
            self.is_started = true;
        }
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), Error> {
        if self.is_started {
            self.inner.set_auto_reconnect(false);
            if let Some(tx) = self.tx.take() {
                let _r = tx.try_send(ClientEvent::Shutdown);
            }
            if let Some(th) = self.th.take() {
                let _r = th.await;
            }
            self.is_started = false;
        }
        Ok(())
    }

    pub fn send<T: Into<ClientEvent>>(&self, ev: T) {
        if let Some(ref tx) = self.tx {
            let _r = tx.try_send(ev.into());
        }
    }
}

pub enum ClientEvent {
    Idle,
    Shutdown,
    Packet(Arc<Packet>),
}

pub trait ClientHandler: Send + Sync {
    fn on_state(&self, state: ClientState);
    fn on_packet(&self, packet: Arc<Packet>);
}

#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum ClientState {
    Connected,
    ConnectError,
    ConnectTimeout,
    Disconnected,
    TryReconnect,
}
