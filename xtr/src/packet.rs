use bitflags::bitflags;
use bytes::{Buf, BufMut, BytesMut};
use std::fmt;

use crate::PackedValues;

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
    pub seq: u32,
    pub ts: u64,
}

impl PacketHead {
    pub fn new(length: u32, type_: PacketType, flags: PacketFlags, stream_id: u32) -> Self {
        Self {
            length,
            type_,
            flags,
            stream_id,
            seq: 0,
            ts: 0,
        }
    }

    pub fn parse(mut data: &[u8]) -> Result<PacketHead, PacketError> {
        if data.len() < 24 {
            return Err(PacketError::NotEnoughData);
        }
        let length = data.get_u32();
        let type_ = PacketType::from(data.get_u8());
        let flags = PacketFlags::from(data.get_u8());
        let stream_id = data.get_u32();
        let seq = data.get_u32();
        let ts = data.get_u64();
        Ok(Self {
            length,
            type_,
            flags,
            stream_id,
            seq,
            ts,
        })
    }

    pub fn to_bytes(&self) -> [u8; 24] {
        let mut bytes = [0u8; 24];
        bytes[0..4].copy_from_slice(&self.length.to_be_bytes());
        bytes[4] = self.type_ as u8;
        bytes[5] = self.flags.bits() as u8;
        bytes[6..10].copy_from_slice(&self.stream_id.to_be_bytes());
        bytes[10..14].copy_from_slice(&self.seq.to_be_bytes());
        bytes[14..22].copy_from_slice(&self.ts.to_be_bytes());
        bytes
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

    pub fn with_packed_values(pv: PackedValues, flags: PacketFlags, stream_id: u32) -> Self {
        let bytes = pv.as_bytes();
        let head = PacketHead::new(
            bytes.len() as u32,
            PacketType::PackedValues,
            flags,
            stream_id,
        );
        let data = BytesMut::from(pv.as_bytes());
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

    pub fn seq(&self) -> u32 {
        self.head.seq
    }

    pub fn set_seq(&mut self, seq: u32) {
        self.head.seq = seq;
    }

    pub fn ts(&self) -> u64 {
        self.head.ts
    }

    pub fn set_ts(&mut self, ts: u64) {
        self.head.ts = ts;
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
        const READONLY = 0x40;
        const ALL = Self::END_STREAM.bits | Self::END_HEADERS.bits | Self::PADDED.bits | Self::PRIORITY.bits | Self::READONLY.bits;
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
    Json = 11,
    PackedValues = 12,
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
            10 => BinCode,
            11 => Json,
            12 => PackedValues,
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
