use super::{Packet, PacketReader};
use log::{debug, error, trace};
use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;

type ClientEventSender = Sender<ClientEvent>;
type ClientEventReceiver = Receiver<ClientEvent>;

type ShutdownSender = oneshot::Sender<()>;
type ShutdownReceiver = oneshot::Receiver<()>;

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
    shutdown_tx: Option<ShutdownSender>,
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
            shutdown_tx: None,
            is_started: false,
        }
    }

    async fn read_loop(inner: Arc<ClientInner>, mut reader: OwnedReadHalf) {
        let mut ticker = tokio::time::interval(Duration::from_millis(100));
        let mut packet_reader = FramedRead::new(&mut reader, PacketReader::new());

        'outer: loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if inner.is_exit_loop() {
                        debug!("检测到退出标志, 终止接收");
                        break;
                    }
                }
                Some(Ok(packet)) = packet_reader.next() => {
                    trace!("收到数据头: {:?}", packet.head);
                    inner.handler.on_packet(Arc::new(packet));
                }
                else => {
                    debug!("检测到接收队列异常, 终止接收");
                    break 'outer;
                }
            }
        }
        inner.set_exit_loop(true);
        debug!("已经退出数据接收循环");
    }

    async fn write_loop(
        inner: Arc<ClientInner>,
        mut rx: Receiver<ClientEvent>,
        mut shutdown_rx: ShutdownReceiver,
        mut writer: OwnedWriteHalf,
    ) -> (ClientEventReceiver, ShutdownReceiver) {
        'outer: loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    debug!("检测到退出标志, 终止发送");
                    break;
                }
                Some(ev) = rx.recv() => {
                    match ev {
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
                            debug!("检测到退出标志, 终止发送");
                            break 'outer;
                        }
                    }
                }
                else => {
                    debug!("检测到发送队列异常, 终止发送");
                    break 'outer;
                }
            }
        }
        inner.set_exit_loop(true);
        debug!("已经退出数据发送循环");
        (rx, shutdown_rx)
    }

    async fn mantain_loop(
        inner: Arc<ClientInner>,
        rx: ClientEventReceiver,
        shutdown_rx: ShutdownReceiver,
    ) {
        let mut rx: Option<ClientEventReceiver> = Some(rx);
        let mut shutdown_rx: Option<ShutdownReceiver> = Some(shutdown_rx);
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
                        shutdown_rx.take().unwrap(),
                        writer,
                    ));
                    let (_, r) = tokio::join!(t1, t2);
                    (rx, shutdown_rx) = r.map(|x| (Some(x.0), Some(x.1))).unwrap();
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
            let (stx, srx) = oneshot::channel();
            let cloned_inner = Arc::clone(&self.inner);
            let th = tokio::task::spawn(async move {
                let _r = Self::mantain_loop(cloned_inner, rx, srx).await;
            });
            self.th = Some(th);
            self.tx = Some(tx);
            self.shutdown_tx = Some(stx);
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
            if let Some(tx) = self.shutdown_tx.take() {
                let _r = tx.send(());
            }
            if let Some(th) = self.th.take() {
                let _r = th.await;
            }
            self.is_started = false;
        }
        Ok(())
    }

    pub fn send<T: Into<ClientEvent>>(&self, ev: T) {
        if self.is_started {
            if let Some(ref tx) = self.tx {
                let _r = tx.try_send(ev.into());
            }
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
