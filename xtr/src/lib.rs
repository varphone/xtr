mod client;
mod packet;
mod server;
mod session;

pub use client::{Client, ClientHandler, ClientState};
pub use packet::{PacketHead, Packet, PacketFlags, PacketType};
pub use server::Server;
pub use session::{SeesionPacketHandler, Session};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
