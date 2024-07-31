use super::{Packet, PacketReader};
use log::{debug, error, trace};
use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    reconnect_delay_ms: AtomicU64,
    timeout_ms: AtomicU64,
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

    fn reconnect_delay_ms(&self) -> u64 {
        self.reconnect_delay_ms.load(Ordering::SeqCst)
    }

    fn set_reconnect_delay_ms(&self, reconnect_delay_ms: u64) {
        self.reconnect_delay_ms
            .store(reconnect_delay_ms, Ordering::SeqCst);
    }

    fn timeout_ms(&self) -> u64 {
        self.timeout_ms.load(Ordering::SeqCst)
    }

    fn set_timeout_ms(&self, timeout_ms: u64) {
        self.timeout_ms.store(timeout_ms, Ordering::SeqCst);
    }
}

/// 一个代表客户端的类型。
pub struct Client {
    inner: Arc<ClientInner>,
    th: Option<JoinHandle<()>>,
    tx: Option<ClientEventSender>,
    shut_reader: Option<ShutdownSender>,
    shut_writer: Option<ShutdownSender>,
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
                reconnect_delay_ms: AtomicU64::new(1000),
                timeout_ms: AtomicU64::new(3000),
            }),
            th: None,
            tx: None,
            shut_reader: None,
            shut_writer: None,
            is_started: false,
        }
    }

    async fn read_loop(
        inner: Arc<ClientInner>,
        mut shutdown_rx: ShutdownReceiver,
        mut reader: OwnedReadHalf,
    ) -> ShutdownReceiver {
        let mut ticker = tokio::time::interval(Duration::from_millis(100));
        let mut packet_reader = FramedRead::new(&mut reader, PacketReader::new());

        let mut last_ts = tokio::time::Instant::now();

        'outer: loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    debug!("The immediate shutdown signal received");
                    break;
                }
                _ = ticker.tick() => {
                    if inner.is_exit_loop() {
                        debug!("The exit loop flag detected");
                        break;
                    }
                    if last_ts.elapsed() > Duration::from_millis(inner.timeout_ms()) {
                        debug!("The peer seems to be inactive for a long time");
                        break;
                    }
                }
                Some(v) = packet_reader.next() => {
                    match v {
                        Ok(packet) => {
                            trace!("Received packet: {:?}", packet.head);
                            last_ts = tokio::time::Instant::now();
                            inner.handler.on_packet(Arc::new(packet));
                        }
                        Err(err) => {
                            error!("Error occurred while receiving body: {}", err);
                            break 'outer;
                        }
                    }
                }
                else => {
                    debug!("The socket read half has encountered an error");
                    break 'outer;
                }
            }
        }
        inner.set_exit_loop(true);
        debug!("The data receiving loop exited");

        shutdown_rx
    }

    async fn set_proto_version(
        inner: &ClientInner,
        writer: &mut OwnedWriteHalf,
    ) -> Result<(), Error> {
        use tokio::time::timeout;
        log::info!("Setting proto version to {}", crate::PROTO_VERSION);
        let packet = Packet::with_proto_version(crate::PROTO_VERSION);
        let tx_tmo = Duration::from_millis(inner.timeout_ms());
        let head_bytes = packet.head.to_bytes();
        let _ = timeout(tx_tmo, writer.write_all(&head_bytes)).await?;
        let body_bytes = packet.data.as_ref();
        let _ = timeout(tx_tmo, writer.write_all(body_bytes)).await?;
        log::info!("Proto version has been set");
        Ok(())
    }

    async fn write_loop(
        inner: Arc<ClientInner>,
        mut rx: Receiver<ClientEvent>,
        mut shutdown_rx: ShutdownReceiver,
        mut writer: OwnedWriteHalf,
    ) -> (ClientEventReceiver, ShutdownReceiver) {
        use tokio::time::timeout;

        if Self::set_proto_version(&inner, &mut writer).await.is_err() {
            return (rx, shutdown_rx);
        }

        let mut ticker = tokio::time::interval(Duration::from_millis(100));

        'outer: loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    debug!("The immediate shutdown singal received");
                    break;
                }
                _ = ticker.tick() => {
                    if inner.is_exit_loop() {
                        debug!("The exit loop flag detected");
                        break;
                    }
                }
                Some(ev) = rx.recv() => {
                    match ev {
                        ClientEvent::Idle => {}
                        ClientEvent::Packet(packet) => {
                            let tx_tmo = Duration::from_millis(inner.timeout_ms());
                            let head_bytes = packet.head.to_bytes();
                            if let Err(err) = timeout(tx_tmo, writer.write_all(&head_bytes)).await {
                                error!("Error occurred while sending head: {}", err);
                                break 'outer;
                            }
                            let body_bytes = packet.data.as_ref();
                            if let Err(err) = timeout(tx_tmo, writer.write_all(body_bytes)).await {
                                error!("Error occurred while sending body: {}", err);
                                break 'outer;
                            }
                        }
                        ClientEvent::Shutdown => {
                            debug!("The ClientEvent::Shutdown received");
                            break 'outer;
                        }
                    }
                }
                else => {
                    debug!("The send queue has encountered an error");
                    break 'outer;
                }
            }
        }

        inner.set_exit_loop(true);
        debug!("The data sending loop exited");

        (rx, shutdown_rx)
    }

    async fn mantain_loop(
        inner: Arc<ClientInner>,
        rx: ClientEventReceiver,
        shut_reader: ShutdownReceiver,
        shut_writer: ShutdownReceiver,
    ) {
        use tokio::time::{sleep, timeout};

        let mut rx: Option<ClientEventReceiver> = Some(rx);
        let mut shut_reader: Option<ShutdownReceiver> = Some(shut_reader);
        let mut shut_writer: Option<ShutdownReceiver> = Some(shut_writer);

        'outer: loop {
            let r = timeout(Duration::from_millis(100), TcpStream::connect(&inner.addr)).await;
            match r {
                Ok(Ok(socket)) => {
                    debug!("Connected to: {}", inner.addr);
                    inner.handler.on_state(ClientState::Connected);
                    inner.set_exit_loop(false);
                    // 优化小包传输
                    socket.set_nodelay(true).unwrap();
                    //
                    let (reader, writer) = socket.into_split();
                    let t1 = tokio::task::spawn(Self::read_loop(
                        Arc::clone(&inner),
                        shut_reader.take().unwrap(),
                        reader,
                    ));
                    let t2 = tokio::task::spawn(Self::write_loop(
                        Arc::clone(&inner),
                        rx.take().unwrap(),
                        shut_writer.take().unwrap(),
                        writer,
                    ));
                    let (r1, r2) = tokio::join!(t1, t2);
                    shut_reader = r1.map(Some).unwrap();
                    (rx, shut_writer) = r2.map(|x| (Some(x.0), Some(x.1))).unwrap();
                    debug!("All data transmission has exited");
                    inner.handler.on_state(ClientState::Disconnected);
                }
                Ok(Err(err)) => {
                    debug!("Connect to {} failed: {}", inner.addr, err);
                    if !inner.is_auto_reconnect() {
                        inner.handler.on_state(ClientState::ConnectError);
                        break 'outer;
                    }
                    inner.handler.on_state(ClientState::TryReconnect);
                }
                Err(err) => {
                    debug!("Connect to {} failed: {}", inner.addr, err);
                    if !inner.is_auto_reconnect() {
                        inner.handler.on_state(ClientState::ConnectTimeout);
                        break 'outer;
                    }
                    inner.handler.on_state(ClientState::TryReconnect);
                }
            }
            if inner.is_auto_reconnect() {
                let delay = Duration::from_millis(inner.reconnect_delay_ms());
                sleep(delay).await;
            } else {
                break 'outer;
            }
        }

        debug!("The maintenance loop exited!");
    }

    pub fn set_auto_reconnect(&self, yes: bool) {
        self.inner.set_auto_reconnect(yes);
    }

    pub fn set_reconnect_delay_ms(&self, reconnect_delay_ms: u64) {
        self.inner.set_reconnect_delay_ms(reconnect_delay_ms);
    }

    pub fn set_timeout_ms(&self, timeout_ms: u64) {
        self.inner.set_timeout_ms(timeout_ms);
    }

    pub async fn start(&mut self) -> Result<(), Error> {
        if !self.is_started {
            let (tx, rx) = channel(100);
            let (sr_tx, sr_rx) = oneshot::channel();
            let (sw_tx, sw_rx) = oneshot::channel();
            let cloned_inner = Arc::clone(&self.inner);
            let th = tokio::task::spawn(async move {
                Self::mantain_loop(cloned_inner, rx, sr_rx, sw_rx).await;
            });
            self.th = Some(th);
            self.tx = Some(tx);
            self.shut_reader = Some(sr_tx);
            self.shut_writer = Some(sw_tx);
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
            if let Some(tx) = self.shut_reader.take() {
                let _r = tx.send(());
            }
            if let Some(tx) = self.shut_writer.take() {
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

/// 一个代表客户端事件的枚举。
pub enum ClientEvent {
    Idle,
    Shutdown,
    Packet(Arc<Packet>),
}

/// 一个代表客户端回调的契定。
pub trait ClientHandler: Send + Sync {
    fn on_state(&self, state: ClientState);
    fn on_packet(&self, packet: Arc<Packet>);
}

/// 一个代表客户端状态的枚举。
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ClientState {
    /// 连接成功。
    Connected,
    /// 连接出现异常。
    ConnectError,
    /// 连接超时。
    ConnectTimeout,
    /// 已经断开连接。
    Disconnected,
    /// 尝试重新连接。
    TryReconnect,
}
