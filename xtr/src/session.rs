use super::Packet;
use std::net::TcpStream;
use std::sync::Arc;

pub struct Session {
    socket: TcpStream,
}

impl Session {
    pub fn new(socket: TcpStream) -> Self {
        Self { socket }
    }
}

pub trait SeesionHandler: Send + Sync {
    fn on_packet(&self, session: &Arc<Session>, packet: Arc<Packet>);
    fn on_state(&self, session: &Arc<Session>, state: SessionState);
}
