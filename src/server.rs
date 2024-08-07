#[allow(unused_imports)]
use super::{Packet, PacketError, PacketFlags, PacketHead, PacketReader, PacketType, Timestamp};
use bytes::Buf;
#[cfg(feature = "fullv")]
use fv_common::VideoFrame;
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::io::{Cursor, Error};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle as TaskJoinHandle;
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;

type ServerSender = Sender<ServerEvent>;
type ServerReceiver = Receiver<ServerEvent>;
use tokio::sync::mpsc::channel as server_channel;

type SessionSender = Sender<SessionEvent>;
type SessionReceiver = Receiver<SessionEvent>;
use tokio::sync::mpsc::channel as session_channel;

// 默认屏蔽流的掩码
const DEFAULT_STREAM_MASK: u64 = 0x5400_0000;

/// 一个代表屏蔽流信息的类型。
#[derive(Debug)]
pub struct MaskStream {
    pub ssid: Option<SessionId>,
    pub stream_id: u32,
    pub masked: bool,
}

/// 一个代表服务器会话的类型。
pub struct Session {
    tx: SessionSender,
    stream_mask: AtomicU64,
    established_ts: u64,
    task: TaskJoinHandle<()>,
}

impl Session {
    /// 清除指定的视频流屏蔽标志。
    /// @param stream_id 要清除的视频流 ID。
    #[allow(dead_code)]
    pub fn clear_stream_mask(&self, stream_id: u32) {
        let mask = 1u64 << stream_id;
        self.stream_mask.fetch_and(!mask, Ordering::SeqCst);
    }

    /// 设置指定的视频流屏蔽标志。
    /// @param stream_id 要设置的视频流 ID。
    /// @note 如果指定的视频流 ID 被屏蔽，则不会发送此流到客户端。
    #[allow(dead_code)]
    pub fn set_stream_mask(&self, stream_id: u32) {
        if stream_id < 64 {
            let mask = 1u64 << stream_id;
            self.stream_mask.fetch_or(mask, Ordering::SeqCst);
        }
    }

    /// 测试指定的视频流是否被屏蔽。
    /// @return 如果视频流被屏蔽则返回 true，否则返回 false。
    #[allow(dead_code)]
    pub fn test_stream_mask(&self, stream_id: u32) -> bool {
        if stream_id < 64 {
            let mask = 1u64 << stream_id;
            let curr = self.stream_mask.load(Ordering::SeqCst);
            (curr & mask) == mask
        } else {
            false
        }
    }
}

struct SessionCtx {
    handler: Arc<dyn SessionHandler>,
    id: SessionId,
    tx: Sender<ServerEvent>,
    is_exit_loop: AtomicBool,
    proto_version: AtomicU32,
}

impl SessionCtx {
    fn is_exit_loop(&self) -> bool {
        self.is_exit_loop.load(Ordering::SeqCst)
    }

    fn set_exit_loop(&self, yes: bool) {
        self.is_exit_loop.store(yes, Ordering::SeqCst)
    }

    #[allow(dead_code)]
    fn proto_version(&self) -> u32 {
        self.proto_version.load(Ordering::SeqCst)
    }

    #[allow(dead_code)]
    fn set_proto_version(&self, version: u32) {
        self.proto_version.store(version, Ordering::SeqCst)
    }

    #[allow(dead_code)]
    fn parse_settings(&self, data: &[u8]) {
        if data.len() < 6 {
            return;
        }
        let mut cursor = Cursor::new(data);
        let id = cursor.get_u16();
        if id == 0x0000 {
            let version = cursor.get_u32();
            self.set_proto_version(version);
            log::debug!("{:?} 设置协议版本为: 0x{:08X}", self.id, version);
        }
    }
}

/// 一个代表服务器会话标识的类型。
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct SessionId(SocketAddr);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 一个代表服务器会话状态的枚举。
#[derive(Copy, Clone, Debug)]
pub enum SessionState {
    Connected,
    ConnectError,
    Disconnected,
}

/// 一个代表服务器会话回调的锲定。
pub trait SessionHandler: Send + Sync {
    fn on_packet(&self, session: &SessionId, packet: Arc<Packet>);
    fn on_state(&self, session: &SessionId, state: SessionState);
}

/// 一个代表服务器会话事件的枚举。
#[allow(dead_code)]
pub enum SessionEvent {
    Idle,
    Packet(Arc<Packet>),
    Shutdown,
    #[cfg(feature = "fullv")]
    VideoFrame(VideoFrame),
    #[cfg(feature = "fullv")]
    VideoFrameEx(VideoFrame, u32, PacketType),
}

/// 一个代表服务器事件的枚举。
pub enum ServerEvent {
    Idle,
    Packet {
        packet: Arc<Packet>,
        ssid: Option<SessionId>,
    },
    SessionClosed(SessionId),
    Shutdown,
    MaskStream(MaskStream),
    #[cfg(feature = "fullv")]
    VideoFrame {
        frame: VideoFrame,
        ssid: Option<SessionId>,
    },
    #[cfg(feature = "fullv")]
    VideoFrameEx {
        frame: VideoFrame,
        ssid: Option<SessionId>,
        stream_id: u32,
        type_: PacketType,
    },
}

struct ServerInner {
    addr: SocketAddr,
    handler: Arc<dyn SessionHandler>,
    tx: ServerSender,
    rx: ServerReceiver,
}

impl ServerInner {
    fn new(
        addr: SocketAddr,
        handler: Arc<dyn SessionHandler>,
        tx: ServerSender,
        rx: ServerReceiver,
    ) -> Self {
        Self {
            addr,
            handler,
            tx,
            rx,
        }
    }

    async fn read_loop(ctx: Arc<SessionCtx>, mut reader: OwnedReadHalf) {
        let mut ticker = tokio::time::interval(Duration::from_millis(100));
        let packet_reader = FramedRead::new(&mut reader, PacketReader::new());
        tokio::pin!(packet_reader);

        'outer: loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if ctx.is_exit_loop() {
                        debug!("{:?} 检测到退出标志, 终止接收", ctx.id);
                        break;
                    }
                }
                frame = packet_reader.next() => {
                    match frame {
                        Some(Ok(packet)) => {
                            trace!("{:?} 收到数据头: {:?}", ctx.id, packet.head);
                            // 处理设置包
                            if packet.stream_id() == crate::packet::XTR_INNER_STREAM_ID_SETTINGS && packet.type_() == PacketType::Settings {
                                ctx.parse_settings(packet.data.as_ref());
                            }
                            // 处理数据包
                            ctx.handler.on_packet(&ctx.id, Arc::new(packet));
                        }
                        Some(Err(err)) => {
                            error!("{:?} 读取数据帧时发生异常: {}", ctx.id, err);
                            break 'outer;
                        }
                        None => {
                            debug!("{:?} 检测到接收队列异常, 终止接收", ctx.id);
                            break 'outer;
                        }
                    }
                }
            }
        }
        ctx.set_exit_loop(true);
        debug!("{:?} 已经退出数据接收循环", ctx.id);
    }

    #[cfg(feature = "fullv")]
    async fn write_video_frame(
        writer: &mut OwnedWriteHalf,
        frame: VideoFrame,
        stream_id: u32,
        type_: PacketType,
    ) -> Result<(), std::io::Error> {
        if let Ok(m) = frame.buffer.map_readable() {
            let pixels = m.as_slice();
            let pixel_bytes = pixels.len() as u32;
            if pixel_bytes == 0 {
                return Ok(());
            } else if pixel_bytes > super::packet::XTR_MAX_PACKET_SIZE {
                return Err(Error::new(
                    std::io::ErrorKind::Other,
                    "视频帧数据过大，无法发送",
                ));
            }
            let mut head =
                PacketHead::new(pixels.len() as u32, type_, PacketFlags::empty(), stream_id);
            head.seq = frame.buffer.offset() as u32;
            head.ts = frame.pts.as_micros();
            let head_bytes = head.to_bytes();
            let mut head_bytes_cursor = Cursor::new(head_bytes);
            let mut pixels_cursor = Cursor::new(pixels);
            let tmo_ms = if pixels.len() >= 1310720 { 3000 } else { 1000 };
            let tx_tmo = Duration::from_millis(tmo_ms);
            let r =
                tokio::time::timeout(tx_tmo, writer.write_all_buf(&mut head_bytes_cursor)).await;
            match r {
                Ok(Ok(_)) => {}
                Ok(Err(err)) => {
                    error!("#{:?} 发送视频帧头时发生异常: {}", stream_id, err);
                    return Err(err);
                }
                Err(err) => {
                    error!("#{:?} 发送视频帧头时超时", stream_id);
                    return Err(err.into());
                }
            }
            let r = tokio::time::timeout(tx_tmo, writer.write_all_buf(&mut pixels_cursor)).await;
            match r {
                Ok(Ok(_)) => {}
                Ok(Err(err)) => {
                    error!("#{:?} 发送视频帧内容时发生异常: {}", stream_id, err);
                    return Err(err);
                }
                Err(err) => {
                    error!("#{:?} 发送视频帧内容时超时", stream_id);
                    return Err(err.into());
                }
            }
        }
        Ok(())
    }

    async fn write_loop(ctx: Arc<SessionCtx>, mut rx: SessionReceiver, mut writer: OwnedWriteHalf) {
        use tokio::time::timeout;

        let tmo = Duration::from_millis(40);

        'outer: loop {
            if ctx.is_exit_loop() {
                warn!("{:?} 数据接收循环已经退出, 终止发送", ctx.id);
                break;
            }
            match timeout(tmo, rx.recv()).await {
                Ok(Some(ev)) => match ev {
                    SessionEvent::Idle => {}
                    SessionEvent::Packet(packet) => {
                        let head_bytes = packet.head.to_bytes();
                        let mut head_bytes_cursor = Cursor::new(&head_bytes);
                        let tx_tmo = Duration::from_millis(1000);
                        match tokio::time::timeout(
                            tx_tmo,
                            writer.write_all_buf(&mut head_bytes_cursor),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {}
                            Ok(Err(err)) => {
                                error!("{:?} 发送数据帧头时发生异常: {}", ctx.id, err);
                                break 'outer;
                            }
                            Err(_) => {
                                error!("{:?} 发送数据帧头时超时", ctx.id);
                                break 'outer;
                            }
                        }
                        let body_bytes = packet.data.as_ref();
                        let mut body_bytes_cursor = Cursor::new(&body_bytes);
                        match tokio::time::timeout(
                            tx_tmo,
                            writer.write_all_buf(&mut body_bytes_cursor),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {}
                            Ok(Err(err)) => {
                                error!("{:?} 发送数据帧内容时发生异常: {}", ctx.id, err);
                                break 'outer;
                            }
                            Err(_) => {
                                error!("{:?} 发送数据帧内容时超时", ctx.id);
                                break 'outer;
                            }
                        }
                    }
                    SessionEvent::Shutdown => {
                        break 'outer;
                    }
                    #[cfg(feature = "fullv")]
                    SessionEvent::VideoFrame(frame) => {
                        if let Err(err) =
                            Self::write_video_frame(&mut writer, frame, 2, PacketType::Data).await
                        {
                            error!("{:?} 发送视频帧时发生异常: {}", ctx.id, err);
                            break 'outer;
                        }
                    }
                    #[cfg(feature = "fullv")]
                    SessionEvent::VideoFrameEx(frame, stream_id, type_) => {
                        // 为了兼容旧版客户端，如果客户端版本低于 0x0001_0001，则发送类型为 Data 的视频帧
                        let typ = if ctx.proto_version() < 0x0001_0001 {
                            PacketType::Data
                        } else {
                            type_
                        };
                        if let Err(err) =
                            Self::write_video_frame(&mut writer, frame, stream_id, typ).await
                        {
                            error!("{:?} 发送视频帧时发生异常: {}", ctx.id, err);
                            break 'outer;
                        }
                    }
                },
                Ok(None) => {
                    error!("{:?} 数据输出队列已经关闭", ctx.id);
                    break 'outer;
                }
                Err(_) => {
                    // trace!("读取数据输出队列时超时");
                }
            }
        }
        ctx.set_exit_loop(true);
        debug!("{:?} 已经退出数据发送循环", ctx.id);
    }

    async fn session_loop(
        socket: TcpStream,
        id: SessionId,
        tx: ServerSender,
        rx: SessionReceiver,
        handler: Arc<dyn SessionHandler>,
    ) {
        handler.on_state(&id, SessionState::Connected);
        // 优化小包传输
        socket.set_nodelay(true).unwrap();
        //
        let (reader, writer) = socket.into_split();
        let ctx = SessionCtx {
            handler,
            id,
            tx,
            is_exit_loop: AtomicBool::new(false),
            proto_version: AtomicU32::new(0),
        };
        let ctx = Arc::new(ctx);
        let t1 = tokio::task::spawn(Self::read_loop(Arc::clone(&ctx), reader));
        let t2 = tokio::task::spawn(Self::write_loop(Arc::clone(&ctx), rx, writer));
        let _r = t1.await;
        let _r = t2.await;
        debug!("{:?} 数据收发已经全部退出", id);
        ctx.handler.on_state(&id, SessionState::Disconnected);
        let _r = ctx.tx.send(ServerEvent::SessionClosed(id)).await;
    }

    async fn resolve_server_accepted(
        &mut self,
        socket: TcpStream,
        addr: SocketAddr,
        sessions: &mut HashMap<SessionId, Session>,
    ) {
        info!(
            "客户端 {:?} 已经连上，现存会话数量: {}",
            addr,
            sessions.len() + 1
        );
        // 设置无延时发送，避免数据量少时延时很大
        socket.set_nodelay(true).unwrap();
        let server_tx = self.tx.clone();
        let session_id = SessionId(addr);
        let (session_tx, session_rx) = session_channel(100);
        let task = tokio::task::spawn(Self::session_loop(
            socket,
            session_id,
            server_tx,
            session_rx,
            Arc::clone(&self.handler),
        ));
        sessions.insert(
            session_id,
            Session {
                tx: session_tx,
                stream_mask: AtomicU64::new(DEFAULT_STREAM_MASK),
                established_ts: Timestamp::now_monotonic().as_micros(),
                task,
            },
        );
    }

    async fn resolve_server_events(
        &mut self,
        ev: ServerEvent,
        sessions: &mut HashMap<SessionId, Session>,
    ) -> Result<(), std::io::Error> {
        match ev {
            ServerEvent::Idle => {}
            ServerEvent::Shutdown => {
                info!("收到关闭信号，退出循环");
                return Err(std::io::Error::new(std::io::ErrorKind::Other, ""));
            }
            ServerEvent::Packet { packet, ssid } => {
                for (_k, v) in sessions
                    .iter()
                    .filter(|(k, _)| ssid.is_none() || ssid.as_ref() == Some(k))
                {
                    if packet.ts() >= v.established_ts {
                        let packet = Arc::clone(&packet);
                        let _r = v.tx.try_send(SessionEvent::Packet(packet));
                    }
                }
            }
            ServerEvent::SessionClosed(ssid) => {
                if let Some(_ss) = sessions.remove(&ssid) {
                    info!("客户端 {} 已经关闭", ssid);
                    // let _r = _client._task.abort();
                }
            }
            ServerEvent::MaskStream(v) => {
                if let Some(ssid) = v.ssid {
                    if let Some(ss) = sessions.get(&ssid) {
                        if v.masked {
                            ss.set_stream_mask(v.stream_id);
                        } else {
                            ss.clear_stream_mask(v.stream_id);
                        }
                    }
                }
            }
            #[cfg(feature = "fullv")]
            ServerEvent::VideoFrame { frame, ssid } => {
                for (_k, v) in sessions.iter().filter(|(fk, fv)| {
                    (ssid.is_none() || (ssid.as_ref() == Some(fk))) && !fv.test_stream_mask(2)
                }) {
                    if frame.pts.as_micros() >= v.established_ts {
                        let frame = frame.clone();
                        let _r = v.tx.try_send(SessionEvent::VideoFrame(frame));
                    }
                }
            }
            #[cfg(feature = "fullv")]
            ServerEvent::VideoFrameEx {
                frame,
                ssid,
                stream_id,
                type_,
            } => {
                for (_k, v) in sessions.iter().filter(|(fk, fv)| {
                    (ssid.is_none() || (ssid.as_ref() == Some(fk)))
                        && !fv.test_stream_mask(stream_id)
                }) {
                    if frame.pts.as_micros() >= v.established_ts {
                        let frame = frame.clone();
                        let _r =
                            v.tx.try_send(SessionEvent::VideoFrameEx(frame, stream_id, type_));
                    }
                }
            }
        }

        Ok(())
    }

    async fn run(&mut self) {
        let socket = tokio::net::TcpSocket::new_v4().unwrap();
        socket.set_reuseaddr(true).unwrap();
        socket.bind(self.addr).unwrap();
        let listener = socket.listen(1024).unwrap();
        let mut sessions: HashMap<SessionId, Session> = HashMap::new();

        info!("正在监听 {} 等待接入", self.addr);

        'outer: loop {
            tokio::select! {
                Some(ev) = self.rx.recv() => {
                    if let Err(_err) = self.resolve_server_events(ev, &mut sessions).await {
                        break 'outer;
                    }
                }
                Ok((socket, addr)) = listener.accept() => {
                    self.resolve_server_accepted(socket, addr, &mut sessions).await
                }
            }
        }

        // 发送关闭消息给客户端线程
        for (id, ss) in sessions.into_iter() {
            debug!("{:?} 发出关闭信号", id);
            let _r = ss.tx.try_send(SessionEvent::Shutdown);
            let _r = ss.task.await;
            debug!("{:?} 会话已经关闭", id);
        }

        info!("连接监听线程已经退出");
    }
}

/// 一个代表服务器的类型。
pub struct Server {
    th: Option<TaskJoinHandle<()>>,
    tx: Option<ServerSender>,
}

impl Server {
    pub async fn new<A: ToSocketAddrs>(addr: A, handler: Arc<dyn SessionHandler>) -> Self {
        let addr = addr
            .to_socket_addrs()
            .map(|mut x| x.next().unwrap())
            .unwrap();
        let (tx, rx) = server_channel(200);
        let cloned_tx = tx.clone();
        let mut inner = ServerInner::new(addr, handler, cloned_tx, rx);
        let th = tokio::task::spawn(async move {
            inner.run().await;
        });
        Self {
            th: Some(th),
            tx: Some(tx),
        }
    }

    pub async fn start(&self) -> Result<(), Error> {
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), Error> {
        if let Some(tx) = self.tx.as_ref() {
            let _ = tx.send(ServerEvent::Shutdown).await;
        }
        Ok(())
    }

    pub fn blocking_send<T: Into<ServerEvent>>(&self, ev: T) {
        if let Some(tx) = self.tx.as_ref() {
            let _r = tx.blocking_send(ev.into());
        }
    }

    pub async fn send<T: Into<ServerEvent>>(&self, ev: T) {
        if let Some(tx) = self.tx.as_ref() {
            let _r = tx.send(ev.into()).await;
        }
    }

    pub fn try_send<T: Into<ServerEvent>>(&self, ev: T) {
        if let Some(tx) = self.tx.as_ref() {
            let _r = tx.try_send(ev.into());
        }
    }

    pub async fn shutdown(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(ServerEvent::Shutdown).await;
        }
        if let Some(th) = self.th.take() {
            let _ = th.await;
        }
    }

    pub fn transfer(&self, _packet: Arc<Packet>) {}
}
