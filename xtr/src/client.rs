use crate::packet::{PacketError, PacketHead};

use super::{Packet, PacketFlags, PacketType, Session};
use bytes::BytesMut;
use crossbeam::channel::{Receiver, RecvTimeoutError, Sender};
use log::{debug, error, info, trace, warn};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;

type ClientEventSender = Sender<ClientEvent>;

struct ClientInner {
    addr: SocketAddr,
    handler: Arc<dyn ClientHandler>,
    is_auto_reconnect: AtomicBool,
    is_reader_exited: AtomicBool,
    is_writer_exited: AtomicBool,
}

impl ClientInner {
    fn is_auto_reconnect(&self) -> bool {
        self.is_auto_reconnect.load(Ordering::SeqCst)
    }

    fn set_auto_reconnect(&self, yes: bool) {
        self.is_auto_reconnect.store(yes, Ordering::SeqCst);
    }

    fn is_reader_exited(&self) -> bool {
        self.is_reader_exited.load(Ordering::SeqCst)
    }

    fn set_reader_exited(&self, yes: bool) {
        self.is_reader_exited.store(yes, Ordering::SeqCst);
    }

    fn is_writer_exited(&self) -> bool {
        self.is_reader_exited.load(Ordering::SeqCst)
    }

    fn set_writer_exited(&self, yes: bool) {
        self.is_writer_exited.store(yes, Ordering::SeqCst);
    }
}

pub struct Client {
    inner: Arc<ClientInner>,
    th: Option<JoinHandle<()>>,
    tx: Option<ClientEventSender>,
}

impl Client {
    pub fn new<A: ToSocketAddrs>(addr: A, handler: Arc<dyn ClientHandler>) -> Self {
        Self {
            inner: Arc::new(ClientInner {
                addr: addr
                    .to_socket_addrs()
                    .map(|mut x| x.next().unwrap())
                    .unwrap(),
                handler,
                is_auto_reconnect: AtomicBool::new(true),
                is_reader_exited: AtomicBool::new(false),
                is_writer_exited: AtomicBool::new(false),
            }),
            th: None,
            tx: None,
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

    async fn read_loop(inner: Arc<ClientInner>, mut reader: OwnedReadHalf) {
        'outer: loop {
            if inner.is_writer_exited() {
                warn!("数据发送循环已经退出, 终止接收");
                break;
            }
            let mut head = [0u8; 9];
            match reader.read_exact(&mut head).await {
                Ok(_) => {
                    if let Ok(head) = PacketHead::parse(&head) {
                        trace!("收到数据头: {:?}", head);
                        let packet = match head.type_ {
                            PacketType::Data => Self::read_data_frame(&mut reader, head).await,
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
                        debug!("ASDASDASDASDASD");
                    }
                }
                Err(err) => {
                    error!("读取帧头时发生异常 {}", err);
                    break 'outer;
                }
            }
        }
        inner.set_reader_exited(true);
        debug!("已经退出数据接收循环");
    }

    async fn write_loop(
        inner: Arc<ClientInner>,
        rx: Receiver<ClientEvent>,
        mut writer: OwnedWriteHalf,
    ) {
        let tmo = Duration::from_millis(40);
        'outer: loop {
            if inner.is_reader_exited() {
                warn!("数据接收循环已经退出, 终止发送");
                break;
            }
            match rx.recv_timeout(tmo) {
                Ok(ev) => match ev {
                    ClientEvent::Idle => {}
                    ClientEvent::Packet(packet) => {}
                    ClientEvent::Shutdown => {
                        break 'outer;
                    }
                },
                Err(err) => {
                    if RecvTimeoutError::Timeout != err {
                        error!("读取数据输出队列时发生异常: {}", err);
                        break 'outer;
                    }
                }
            }
        }
        inner.set_writer_exited(true);
        debug!("已经退出数据发送循环");
    }

    async fn mantain_loop(inner: Arc<ClientInner>, rx: Receiver<ClientEvent>) {
        'outer: loop {
            let r =
                tokio::time::timeout(Duration::from_millis(100), TcpStream::connect(&inner.addr))
                    .await;
            match r {
                Ok(Ok(socket)) => {
                    debug!("已成功连接到: {}", inner.addr);
                    inner.handler.on_state(ClientState::Connected);
                    // 优化小包传输
                    // socket.set_nodelay(true).unwrap();
                    //
                    let (reader, writer) = socket.into_split();
                    let t1 = tokio::task::spawn(Self::read_loop(Arc::clone(&inner), reader));
                    let t2 = tokio::task::spawn(Self::write_loop(
                        Arc::clone(&inner),
                        rx.clone(),
                        writer,
                    ));
                    t1.await;
                    t2.await;
                    debug!("数据收发已经全部退出");
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
            }
        }
    }

    pub fn start(&mut self) {
        let (tx, rx) = crossbeam::channel::bounded(100);
        let cloned_inner = Arc::clone(&self.inner);
        let th = thread::spawn(move || {
            use tokio::runtime::Builder;
            // let rt = Builder::new_current_thread().enable_all().build().unwrap();
            let rt = Builder::new_multi_thread().enable_all().build().unwrap();
            rt.block_on(Self::mantain_loop(cloned_inner, rx));
        });
        self.th = Some(th);
        self.tx = Some(tx);
    }

    pub fn stop(&mut self) {
        self.inner.set_auto_reconnect(false);
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
pub enum ClientState {
    Connected,
    ConnectError,
    ConnectTimeout,
    Disconnected,
    TryReconnect,
}
