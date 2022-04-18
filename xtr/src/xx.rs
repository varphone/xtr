impl VideoStreamer {
    /// 接收客户端连接。
    async fn accept_connection(
        tx: mpsc::Sender<VideoStreamerEvent>,
        mut rx: mpsc::Receiver<VideoStreamerEvent>,
        listener: TcpListener,
    ) {
        let mut clients: HashMap<SocketAddr, Client> = HashMap::new();
        'outer: loop {
            tokio::select! {
                Some(ev) = rx.recv() => {
                    match ev {
                        VideoStreamerEvent::Shutdown => {
                            info!("收到关闭信号，退出循环");
                            break 'outer;
                        }
                        VideoStreamerEvent::ClientClosed(addr) => {
                            if let Some(_client) = clients.remove(&addr) {
                                info!("客户端 {:?} 已经关闭", addr);
                                // let _r = _client._task.abort();
                            }
                        }
                        VideoStreamerEvent::NewFrame(frame) => {
                            for (_k, v) in clients.iter() {
                                let _r = v.tx.try_send(frame.clone());
                            }
                        }
                    }
                }
                Ok((mut socket, addr)) = listener.accept() => {
                    info!("客户端 {:?} 已经连上", addr);
                    // 设置无延时发送，避免数据量少时延时很大
                    socket.set_nodelay(true).unwrap();
                    let tx = tx.clone();
                    let (client_tx, mut client_rx) = mpsc::channel::<VideoFrame>(30);
                    let task = tokio::task::spawn(async move {
                        let mut wait_for_keyframe = true;
                        while let Some(frame) = client_rx.recv().await {
                            if let Ok(map) = frame.buffer.map_readable() {
                                let bytes = map.as_slice();
                                if wait_for_keyframe {
                                    let nalu_type = bytes[4] >> 1;
                                    if nalu_type == 32 {
                                        wait_for_keyframe = false;
                                    } else {
                                        trace!("等待关键帧给客户端 {:?}", addr);
                                        continue;
                                    }
                                }
                                if let Err(err) = socket.write_all(bytes).await {
                                    warn!("发送码流到 {:?} 发生异常: {}", addr, err);
                                    break;
                                }
                            }
                        }
                        let _r = tx.try_send(VideoStreamerEvent::ClientClosed(addr));
                    });
                    clients.insert(addr, Client { tx: client_tx, _task: task });
                }
            }
        }
        info!("连接监听线程已经退出");
    }

    /// 创建一个新的视频流发送器实例。
    pub async fn new() -> Result<Self, std::io::Error> {
        let bind_addrs = format!("0.0.0.0:{}", DEFAULT_LIVE_VIDEO_PORT);
        let addr = bind_addrs.parse().unwrap();
        let socket = tokio::net::TcpSocket::new_v4()?;
        socket.set_reuseaddr(true)?;
        socket.bind(addr).unwrap();
        let listener = socket.listen(1024).unwrap();
        let (tx, rx) = mpsc::channel(50);
        let cloned_tx = tx.clone();
        let _thread = tokio::task::spawn(Self::accept_connection(cloned_tx, rx, listener));
        info!("正在监听 {} 等待接入", bind_addrs);
        Ok(Self { tx })
    }

    /// 关闭视频流发送器实例。
    pub fn shutdown(&self) {
        let _r = self.tx.try_send(VideoStreamerEvent::Shutdown);
    }

    /// 向视频流发送器实例发送事件。
    pub fn try_send<T>(&self, ev: T) -> Result<(), mpsc::error::TrySendError<VideoStreamerEvent>>
    where
        T: Into<VideoStreamerEvent>,
    {
        self.tx.try_send(ev.into())
    }
}

impl Drop for VideoStreamer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 一个描述视频流发送器事件的枚举。
#[derive(Debug)]
pub enum VideoStreamerEvent {
    Shutdown,
    ClientClosed(SocketAddr),
    NewFrame(VideoFrame),
}

impl From<VideoFrame> for VideoStreamerEvent {
    fn from(val: VideoFrame) -> Self {
        Self::NewFrame(val)
    }
}
