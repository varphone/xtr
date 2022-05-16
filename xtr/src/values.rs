use bytes::{Buf, BufMut, BytesMut};
use paste::paste;
use std::io::Cursor;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct PackedItem {
    pub addr: u16,
    pub kind: PackedValueKind,
    pub elms: u8,
}

pub struct PackedItemIter<'a> {
    cursor: Cursor<&'a [u8]>,
}

impl<'a> PackedItemIter<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(data),
        }
    }
}

impl<'a> Iterator for PackedItemIter<'a> {
    type Item = PackedItem;
    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor.has_remaining() {
            let n = self.cursor.get_u8();
            let k = self.cursor.get_u8();
            let w = (k & 0x0f) as usize;
            let addr = self.cursor.get_u16();
            self.cursor.advance((n as usize + 1) * w);
            Some(PackedItem {
                addr,
                kind: PackedValueKind::from(k),
                elms: n + 1,
            })
        } else {
            None
        }
    }
}

pub struct PackedValues {
    data: BytesMut,
}

macro_rules! values_impl_type {
    ($t:ty, $p:expr) => {
        paste! {
            #[doc = "获取指定地址 `addr` 类型为 `" $t "` 的值。"]
            pub fn [<get_ $t>](&self, addr: u16) -> Option<$t> {
                let mut s: &[u8] = self.data.as_ref();
                while s.has_remaining() {
                    let n = s.get_u8();
                    let k = s.get_u8();
                    let w = (k & 0x0f) as usize;
                    let curr = s.get_u16();
                    if w as usize != std::mem::size_of::<$t>() {
                        s.advance((n as usize + 1) * w);
                    } else {
                        let v = s.[<get_ $t>]();
                        if n == 0 && curr == addr {
                            return Some(v);
                        } else if n > 0 {
                            for _ in 0..n {
                                let _ = s.[<get_ $t>]();
                            }
                        }
                    }
                }
                None
            }

            #[doc = "获取指定地址 `addr` 类型为 `" $t "` 的多个值。"]
            pub fn [<get_ $t s>](&self, addr: u16, num: u16) -> Option<Vec<$t>> {
                if num == 0 {
                    return None;
                }
                let mut s: &[u8] = self.data.as_ref();
                while s.has_remaining() {
                    let n = s.get_u8();
                    let k = s.get_u8();
                    let w = (k & 0x0f) as usize;
                    let curr = s.get_u16();
                    if w != std::mem::size_of::<$t>() {
                        s.advance((n as usize + 1) * w);
                    } else {
                        if n > 0 && curr == addr {
                            let mut v: Vec<$t> = vec![];
                            for _ in 0..=n.min((num-1) as u8) {
                                v.push(s.[<get_ $t>]());
                            }
                            return Some(v);
                        }
                        s.advance((n as usize + 1) * w);
                    }
                }
                None
            }

            #[doc = "设置指定地址 `addr` 类型为 `" $t "` 的值。"]
            pub fn [<put_ $t>](&mut self, addr: u16, val: $t) {
                self.data.put_u8(0x00);
                self.data.put_u8($p);
                self.data.put_u16(addr);
                self.data.[<put_ $t>](val);
            }

            #[doc = "设置指定地址 `addr` 类型为 `" $t "` 的多个值。"]
            pub fn [<put_ $t s>](&mut self, addr: u16, vals: &[$t]) {
                let m = 0x10000 - addr as u32;
                let n = m.min(vals.len() as u32).min(256);
                if n > 1 {
                    self.data.put_u8((n - 1) as u8);
                    self.data.put_u8($p);
                    self.data.put_u16(addr);
                    for v in &vals[..n as usize] {
                        self.data.[<put_ $t>](*v);
                    }
                }
            }
        }
    };
}

impl PackedValues {
    pub fn new() -> Self {
        Self {
            data: BytesMut::new(),
        }
    }

    pub fn with_bytes(data: &[u8]) -> Self {
        Self {
            data: BytesMut::from(data),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.data.as_ref()
    }

    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        self.data.as_mut()
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn items(&self) -> PackedItemIter<'_> {
        PackedItemIter::new(self.data.as_ref())
    }

    values_impl_type!(i8, 0x81);
    values_impl_type!(i16, 0x82);
    values_impl_type!(i32, 0x84);
    values_impl_type!(i64, 0x88);

    values_impl_type!(u8, 0x01);
    values_impl_type!(u16, 0x02);
    values_impl_type!(u32, 0x04);
    values_impl_type!(u64, 0x08);

    values_impl_type!(f32, 0x44);
    values_impl_type!(f64, 0x48);
}

impl Default for PackedValues {
    fn default() -> Self {
        Self::new()
    }
}

/// 一个代表打包的值的类型的枚举。
#[repr(u8)]
#[derive(Copy, Clone, Debug)]
pub enum PackedValueKind {
    I8 = 0x81,
    I16 = 0x82,
    I32 = 0x84,
    I64 = 0x88,
    U8 = 0x01,
    U16 = 0x02,
    U32 = 0x04,
    U64 = 0x08,
    F32 = 0x44,
    F64 = 0x48,
    Unknown = 0xff,
}

impl From<u8> for PackedValueKind {
    fn from(val: u8) -> Self {
        match val {
            0x81 => Self::I8,
            0x82 => Self::I16,
            0x84 => Self::I32,
            0x88 => Self::I64,
            0x01 => Self::U8,
            0x02 => Self::U16,
            0x04 => Self::U32,
            0x08 => Self::U64,
            0x44 => Self::F32,
            0x48 => Self::F64,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_values() {
        let mut pv = PackedValues::new();
        pv.put_i16(0x0001, 1234);
        pv.put_i16(0x0002, -1234);
        pv.put_u16(0x0003, 43210);
        pv.put_u8s(0x0001, &b"1234567"[..]);
    }

    #[test]
    fn array_values() {
        let mut pv = PackedValues::new();
        let bytes = (i8::MIN..=i8::MAX).collect::<Vec<i8>>();
        pv.put_i8s(0x0000, &bytes[..]);
        for i in 1..256 {
            let r = pv.get_i8s(0x0000, i as u16).unwrap();
            assert_eq!(&r[..i], &bytes[..i]);
        }
        pv.clear();
        let bytes = (u8::MIN..=u8::MAX).collect::<Vec<u8>>();
        pv.put_u8s(0x0000, &bytes[..]);
        for i in 1..256 {
            let r = pv.get_u8s(0x0000, i as u16).unwrap();
            assert_eq!(&r[..i], &bytes[..i]);
        }
    }
}
