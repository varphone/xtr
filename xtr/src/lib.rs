mod client;
mod packet;
mod server;
mod session;
mod values;

pub use client::{Client, ClientEvent, ClientHandler, ClientState};
pub use packet::{Packet, PacketError, PacketFlags, PacketHead, PacketType};
pub use server::Server;
pub use session::{SeesionPacketHandler, Session};
pub use values::{PackedItem, PackedItemIter, PackedValueKind, PackedValues};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
