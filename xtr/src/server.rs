use super::{Packet, SeesionPacketHandler, Session};
use std::{
    net::{TcpStream, ToSocketAddrs},
    sync::Arc,
};

pub struct Server {
    packet_handler: Arc<dyn SeesionPacketHandler>,
    sessions: Vec<Arc<Session>>,
}

impl Server {
    pub fn new<A: ToSocketAddrs>(addr: A, packet_handler: Arc<dyn SeesionPacketHandler>) -> Self {
        Self {
            packet_handler,
            sessions: vec![],
        }
    }

    pub fn start(&self) {}

    pub fn stop(&self) {}

    pub fn transfer(&self, packet: Arc<Packet>) {}
}
