use crate::{PackedValues, Timestamp};
use bitflags::bitflags;
use bytes::{Buf, BytesMut};
use std::fmt;
use tokio_util::codec::Decoder;

pub const XTR_MAX_PACKET_SIZE: u32 = 64 * 1024 * 1024;
pub const XTR_MAX_STREAM_ID: u32 = 0xffff;

/// 一个代表数据包异常的枚举。
#[derive(Debug)]
pub enum PacketError {
    BufferTooLarge(u32, u32),
    InvalidHead,
    InvalidStreamId(u32),
    LengthTooSmall(u32, u32),
    LengthTooLarge(u32, u32),
    NotEnoughData(u32, u32),
    UnknownType(u8),
    Io(std::io::Error),
}

impl From<std::io::Error> for PacketError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl fmt::Display for PacketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PacketError::BufferTooLarge(cur, max) => {
                write!(f, "Buffer Too Large({} > {})", cur, max)
            }
            PacketError::InvalidHead => write!(f, "Invalid Head"),
            PacketError::InvalidStreamId(id) => write!(f, "Invalid Stream Id({})", id),
            PacketError::LengthTooSmall(cur, max) => {
                write!(f, "Length Too Small({} < {})", cur, max)
            }
            PacketError::LengthTooLarge(cur, max) => {
                write!(f, "Length Too Large({} > {})", cur, max)
            }
            PacketError::NotEnoughData(cur, min) => write!(f, "Not Enough Data({} < {})", cur, min),
            PacketError::UnknownType(type_) => write!(f, "Unknown Type({})", type_),
            PacketError::Io(err) => write!(f, "Io({})", err),
        }
    }
}

impl std::error::Error for PacketError {}

/// 一个代表数据包头的类型。
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
            ts: Timestamp::now_monotonic().as_micros(),
        }
    }

    pub fn parse(mut data: &[u8]) -> Result<PacketHead, PacketError> {
        if data.len() < 24 {
            return Err(PacketError::NotEnoughData(data.len() as u32, 24));
        }
        let length = data.get_u32();
        let raw_type = data.get_u8();
        let type_ = PacketType::from(raw_type);
        let flags = PacketFlags::from(data.get_u8());
        let stream_id = data.get_u32();
        let seq = data.get_u32();
        let ts = data.get_u64();
        if length < 4 {
            return Err(PacketError::LengthTooSmall(length, 4));
        } else if length > XTR_MAX_PACKET_SIZE {
            return Err(PacketError::LengthTooLarge(length, XTR_MAX_PACKET_SIZE));
        }
        if type_ == PacketType::Unknown {
            return Err(PacketError::UnknownType(raw_type));
        }
        if stream_id > XTR_MAX_STREAM_ID {
            return Err(PacketError::InvalidStreamId(stream_id));
        }
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
        bytes[5] = self.flags.bits();
        bytes[6..10].copy_from_slice(&self.stream_id.to_be_bytes());
        bytes[10..14].copy_from_slice(&self.seq.to_be_bytes());
        bytes[14..22].copy_from_slice(&self.ts.to_be_bytes());
        bytes
    }
}

/// 一个代表数据包的类型。
#[derive(Clone, Debug)]
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

    pub fn as_bytes(&self) -> &[u8] {
        self.data.as_ref()
    }

    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        self.data.as_mut()
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

    pub fn stream_id(&self) -> u32 {
        self.head.stream_id
    }

    pub fn set_stream_id(&mut self, stream_id: u32) {
        self.head.stream_id = stream_id;
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

    pub fn to_packed_values(&self) -> PackedValues {
        PackedValues::with_bytes(self.as_ref())
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
    /// 一个代表数据包标志位的类型。
    #[derive(Copy, Clone, Debug)]
    #[repr(C)]
    pub struct PacketFlags: u8 {
        const END_STREAM = 0x01;
        const END_HEADERS = 0x04;
        const PADDED = 0x08;
        const PRIORITY = 0x20;
        const READONLY = 0x40;
        const ALL = 0x01 | 0x04 | 0x08 | 0x20 | 0x40;
    }
}

impl From<u8> for PacketFlags {
    fn from(val: u8) -> Self {
        Self::from_bits_truncate(val)
    }
}

impl From<PacketFlags> for u8 {
    fn from(val: PacketFlags) -> Self {
        val.bits()
    }
}

/// 一个代表数据包读取器的类型。
pub struct PacketReader {
    head: Option<PacketHead>,
}

impl PacketReader {
    pub fn new() -> Self {
        Self { head: None }
    }
}

impl Default for PacketReader {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for PacketReader {
    type Item = Packet;
    type Error = PacketError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() as u32 > XTR_MAX_PACKET_SIZE {
            return Err(PacketError::BufferTooLarge(
                src.len() as u32,
                XTR_MAX_PACKET_SIZE,
            ));
        }
        if self.head.is_none() && src.len() >= 24 {
            let data = src.split_to(24);
            let ph = PacketHead::parse(&data)?;
            self.head = Some(ph);
        }
        if let Some(ref ph) = self.head {
            if src.len() >= ph.length as usize {
                let body = src.split_to(ph.length as usize);
                let ph = self.head.take().unwrap();
                return Ok(Some(Packet::with_data(ph, body)));
            }
        }
        Ok(None)
    }
}

/// 一个代表数据包类型的枚举。
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
