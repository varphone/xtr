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

pub trait SeesionPacketHandler: Send + Sync {
    fn on_packet(&self, packet: Arc<Packet>, session: Arc<Session>);
}
