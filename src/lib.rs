mod client;
mod packet;
mod server;
mod utils;
mod values;

pub mod ffi;

pub use client::{Client, ClientEvent, ClientHandler, ClientState};
pub use packet::{Packet, PacketError, PacketFlags, PacketHead, PacketReader, PacketType};
pub use server::{MaskStream, Server, ServerEvent, SessionHandler, SessionId, SessionState};
pub use utils::Timestamp;
pub use values::{
    PackedItem, PackedItemIter, PackedItemMapTo, PackedValueKind, PackedValues, PackedValuesRead,
    PackedValuesWrite,
};

pub const PROTO_VERSION: u32 = 0x0001_0001;
