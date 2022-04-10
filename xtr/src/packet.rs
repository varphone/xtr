use bitflags::bitflags;
use bytes::BytesMut;
use std::fmt;

#[derive(Copy, Clone, Debug)]
pub enum PacketError {
    LengthTooLarge,
    NotEnoughData,
    UnknownType(u8),
}

impl fmt::Display for PacketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PacketError::LengthTooLarge => write!(f, "Length Too Large"),
            PacketError::NotEnoughData => write!(f, "Not Enough Data"),
            PacketError::UnknownType(type_) => write!(f, "Unknown Type({})", type_),
        }
    }
}

impl std::error::Error for PacketError {}

#[derive(Copy, Clone, Debug)]
pub struct PacketHead {
    pub length: u32,
    pub type_: PacketType,
    pub flags: PacketFlags,
    pub stream_id: u32,
}

impl PacketHead {
    pub fn new(length: u32, type_: PacketType, flags: PacketFlags, stream_id: u32) -> Self {
        Self {
            length,
            type_,
            flags,
            stream_id,
        }
    }

    pub fn parse(data: &[u8]) -> Result<PacketHead, PacketError> {
        if data.len() < 9 {
            return Err(PacketError::NotEnoughData);
        }
        let length = u32::from_be_bytes([data[0], data[1], data[2], 0]);
        let type_ = PacketType::from(data[3]);
        let flags = PacketFlags::from(data[4]);
        let stream_id = u32::from_be_bytes([data[5], data[6], data[7], data[8]]);
        Ok(Self {
            length,
            type_,
            flags,
            stream_id,
        })
    }
}

#[repr(C)]
pub struct Packet {
    pub head: PacketHead,
    pub data: BytesMut,
}

impl Packet {
    pub fn alloc_data(head: PacketHead) -> Self {
        let mut data = BytesMut::with_capacity(head.length as usize);
        unsafe {
            data.set_len(head.length as usize);
        }
        Self::with_data(head, data)
    }

    pub fn with_data<T: Into<BytesMut>>(head: PacketHead, data: T) -> Self {
        let data = data.into();
        Self { head, data }
    }

    pub fn as_ptr(&mut self) -> *const u8 {
        self.data.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }

    pub fn flags(&self) -> PacketFlags {
        self.head.flags
    }

    pub fn set_flags(&mut self, val: PacketFlags) {
        self.head.flags = val;
    }

    pub fn length(&self) -> u32 {
        self.head.length
    }

    pub fn set_length(&mut self, length: u32) {
        self.head.length = length;
    }

    pub fn type_(&self) -> PacketType {
        self.head.type_
    }

    pub fn set_type(&mut self, val: PacketType) {
        self.head.type_ = val;
    }
}

impl AsRef<[u8]> for Packet {
    fn as_ref(&self) -> &[u8] {
        &self.data[..]
    }
}

impl AsMut<[u8]> for Packet {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.data[..]
    }
}

bitflags! {
    #[repr(C)]
    pub struct PacketFlags: u8 {
        const END_STREAM = 0x1;
        const END_HEADERS = 0x4;
        const PADDED = 0x8;
        const PRIORITY = 0x20;
        const ALL = Self::END_STREAM.bits | Self::END_HEADERS.bits | Self::PADDED.bits | Self::PRIORITY.bits;
    }
}

impl From<u8> for PacketFlags {
    fn from(val: u8) -> Self {
        Self::from_bits_truncate(val)
    }
}

impl From<PacketFlags> for u8 {
    fn from(val: PacketFlags) -> Self {
        val.bits
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PacketType {
    Data = 0,
    Headers = 1,
    Priority = 2,
    Reset = 3,
    Settings = 4,
    PushPromise = 5,
    Ping = 6,
    GoAway = 7,
    WindowUpdate = 8,
    Continuation = 9,
    BinCode = 10,
    Unknown,
}

impl PacketType {
    pub fn new(typ: u8) -> Self {
        Self::from(typ)
    }
}

impl From<u8> for PacketType {
    fn from(val: u8) -> Self {
        Self::from(val as usize)
    }
}

impl From<u16> for PacketType {
    fn from(val: u16) -> Self {
        Self::from(val as usize)
    }
}

impl From<u32> for PacketType {
    fn from(val: u32) -> Self {
        Self::from(val as usize)
    }
}

impl From<usize> for PacketType {
    fn from(val: usize) -> Self {
        use PacketType::*;
        match val {
            0 => Data,
            1 => Headers,
            2 => Priority,
            3 => Reset,
            4 => Settings,
            5 => PushPromise,
            6 => Ping,
            7 => GoAway,
            8 => WindowUpdate,
            9 => Continuation,
            _ => Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_size() {
        assert_eq!(std::mem::size_of::<PacketFlags>(), 1);
        assert_eq!(std::mem::size_of::<PacketType>(), 1);
    }
}
