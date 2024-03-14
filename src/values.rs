use bytes::{Buf, BufMut, BytesMut};
use paste::paste;
use std::io::Cursor;

/// 一个代表打包的值表的条目的类型。
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct PackedItem {
    /// 值的地址。
    pub addr: u16,
    /// 值的类型。
    pub kind: u8,
    /// 值的个数。
    pub elms: u16,
    /// 条目所处位置。
    pub ipos: u64,
}

/// 一个代表打包的值表的条目的值映射操作的契定。
pub trait PackedItemMapTo<T: ?Sized> {
    /// 如果 `pv` 中存在 `self` 相关的值存在则用其为 `dst` 赋值。
    fn map_to(&self, pv: &PackedValues, dst: &mut T);
}

macro_rules! impl_map_to {
    ($t:ty) => {
        paste! {
            impl PackedItemMapTo<$t> for PackedItem {
                fn map_to(&self, pv: &PackedValues, dst: &mut $t) {
                    self.[<map_to_ $t>](pv, dst);
                }
            }

            impl PackedItemMapTo<[$t]> for PackedItem {
                fn map_to(&self, pv: &PackedValues, dst: &mut [$t]) {
                    self.[<map_to_ $t s>](pv, dst);
                }
            }
        }
    };
}

macro_rules! impl_map_to_typed {
    ($t:ty) => {
        paste! {
            #[doc = "如果 `pv` 中存在 `self` 相关的值存在则用其为 `dst` 赋值。"]
            pub fn [<map_to_ $t>](&self, pv: &PackedValues, dst: &mut $t) {
                pv.peek_to(self.addr, self.ipos as usize, dst);
            }

            #[doc = "如果 `pv` 中存在 `self` 相关的多个值存在则用其为 `dst` 赋值。"]
            pub fn [<map_to_ $t s>](&self, pv: &PackedValues, dst: &mut [$t]) {
                pv.peek_many_to(self.addr, self.ipos as usize, dst);
            }
        }
    };
}

impl PackedItem {
    /// 获取值的地址。
    pub fn addr(&self) -> u16 {
        self.addr
    }

    /// 获取值的类型。
    pub fn kind(&self) -> PackedValueKind {
        PackedValueKind::from(self.kind)
    }

    /// 获取值的个数。
    pub fn elms(&self) -> u16 {
        self.elms
    }

    /// 获取条目所处位置。
    pub fn ipos(&self) -> u64 {
        self.ipos
    }

    impl_map_to_typed!(i8);
    impl_map_to_typed!(i16);
    impl_map_to_typed!(i32);
    impl_map_to_typed!(i64);

    impl_map_to_typed!(u8);
    impl_map_to_typed!(u16);
    impl_map_to_typed!(u32);
    impl_map_to_typed!(u64);

    impl_map_to_typed!(f32);
    impl_map_to_typed!(f64);
}

impl_map_to!(i8);
impl_map_to!(i16);
impl_map_to!(i32);
impl_map_to!(i64);

impl_map_to!(u8);
impl_map_to!(u16);
impl_map_to!(u32);
impl_map_to!(u64);

impl_map_to!(f32);
impl_map_to!(f64);

/// 一个代表打包的值表的条目的迭代器的类型。
pub struct PackedItemIter<'a> {
    cursor: Cursor<&'a [u8]>,
}

impl<'a> PackedItemIter<'a> {
    /// 创建一个打包的值表的条目的迭代器。
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
            let ipos = self.cursor.position();
            let n = self.cursor.get_u8();
            let k = self.cursor.get_u8();
            let w = (k & 0x0f) as usize;
            let addr = self.cursor.get_u16();
            self.cursor.advance((n as usize + 1) * w);
            Some(PackedItem {
                addr,
                kind: k,
                elms: n as u16 + 1,
                ipos,
            })
        } else {
            None
        }
    }
}

/// 一个代表打包的值表的读操作的契定。
pub trait PackedValuesRead<T: Copy + Clone> {
    /// 获取指定地址 `addr` 类型为 `T` 的值。
    fn get(&self, addr: u16) -> Option<T>;

    /// 获取指定地址 `addr` 类型为 `T` 的多个值。
    fn get_many(&self, addr: u16, num: u16) -> Option<Vec<T>>;

    /// 如果指定地址 `addr` 类型为 `T` 的值存在则用其为 `dst` 赋值。
    fn get_to(&self, addr: u16, dst: &mut T) {
        if let Some(v) = self.get(addr) {
            *dst = v;
        }
    }

    /// 如果指定地址 `addr` 类型为 `T` 的多个值存在则用其为 `dst` 赋值。
    fn get_many_to(&self, addr: u16, dst: &mut [T]) {
        if let Some(v) = self.get_many(addr, dst.len() as u16) {
            let n = v.len().min(dst.len());
            dst[..n].copy_from_slice(&v[..n]);
        }
    }

    /// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `T` 的值。
    fn peek(&self, addr: u16, ipos: usize) -> Option<T>;

    /// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `T` 的多个值。
    fn peek_many(&self, addr: u16, num: u16, ipos: usize) -> Option<Vec<T>>;

    /// 如果指定偏移 `ipos` 处地址 `addr` 类型为 `T` 的值存在则用其为 `dst` 赋值。
    fn peek_to(&self, addr: u16, ipos: usize, dst: &mut T) {
        if let Some(v) = self.peek(addr, ipos) {
            *dst = v;
        }
    }

    /// 如果指定地址 `addr` 类型为 `T` 的多个值存在则用其为 `dst` 赋值。
    fn peek_many_to(&self, addr: u16, ipos: usize, dst: &mut [T]) {
        if let Some(v) = self.peek_many(addr, dst.len() as u16, ipos) {
            let n = v.len().min(dst.len());
            dst[..n].copy_from_slice(&v[..n]);
        }
    }
}

macro_rules! impl_read {
    ($t:ty) => {
        paste! {
            impl PackedValuesRead<$t> for PackedValues {
                fn get(&self, addr: u16) -> Option<$t> {
                    self.[<get_ $t>](addr)
                }

                fn get_many(&self, addr: u16, num: u16) -> Option<Vec<$t>> {
                    self.[<get_ $t s>](addr, num)
                }

                fn peek(&self, addr: u16, ipos: usize) -> Option<$t> {
                    self.[<peek_ $t>](addr, ipos)
                }

                fn peek_many(&self, addr: u16, num: u16, ipos: usize) -> Option<Vec<$t>> {
                    self.[<peek_ $t s>](addr, num, ipos)
                }
            }
        }
    };
}

/// 一个代表打包的值表的写操作的契定。
pub trait PackedValuesWrite<T> {
    /// 设置指定地址 `addr` 类型为 `T` 的值。
    fn put(&mut self, addr: u16, val: T);
    /// 设置指定地址 `addr` 类型为 `T` 的多个值。
    fn put_many(&mut self, addr: u16, vals: &[T]);
}

macro_rules! impl_write {
    ($t:ty) => {
        paste! {
            impl PackedValuesWrite<$t> for PackedValues {
                fn put(&mut self, addr: u16, val: $t) {
                    self.[<put_ $t>](addr, val);
                }

                fn put_many(&mut self, addr: u16, vals: &[$t]) {
                    self.[<put_ $t s>](addr, vals);
                }
            }
        }
    };
}

/// 一个代表打包的值表的类型。
#[derive(Clone, Debug)]
pub struct PackedValues {
    data: BytesMut,
}

macro_rules! impl_op_typed {
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

            #[doc = "获取指定偏移 `ipos` 处地址 `addr` 类型为 `" $t "` 的值。"]
            pub fn [<peek_ $t>](&self, addr: u16, ipos: usize) -> Option<$t> {
                let mut s: &[u8] = self.data.as_ref();
                if s.len() < ipos + 2 {
                    return None;
                }
                let rem_bytes = s.len() - ipos;
                s.advance(ipos as usize);
                let n = s.get_u8();
                let k = s.get_u8();
                let w = (k & 0x0f) as usize;
                let curr = s.get_u16();
                let val_size = std::mem::size_of::<$t>();
                let val_bytes = val_size * (n as usize + 1);
                if w as usize != val_size || curr != addr || rem_bytes < val_bytes + 2 {
                    return None;
                }
                Some(s.[<get_ $t>]())
            }

            #[doc = "获取指定偏移 `ipos` 处地址 `addr` 类型为 `" $t "` 的多个值。"]
            pub fn [<peek_ $t s>](&self, addr: u16, num: u16, ipos: usize) -> Option<Vec<$t>> {
                let mut s: &[u8] = self.data.as_ref();
                if num == 0 || s.len() < ipos + 2 {
                    return None;
                }
                let rem_bytes = s.len() - ipos;
                s.advance(ipos as usize);
                let n = s.get_u8();
                let k = s.get_u8();
                let w = (k & 0x0f) as usize;
                let curr = s.get_u16();
                let val_size = std::mem::size_of::<$t>();
                let val_bytes = val_size * (n as usize + 1);
                if w as usize != val_size || curr != addr || rem_bytes < val_bytes + 2 {
                    return None;
                }
                let mut v: Vec<$t> = vec![];
                for _ in 0..=n.min((num-1) as u8) {
                    v.push(s.[<get_ $t>]());
                }
                Some(v)
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
    /// 创建一个打包的值表。
    ///
    /// # 示例
    ///
    /// ```
    /// use xtr::PackedValues;
    ///
    /// let mut pv = PackedValues::new();
    /// pv.put_u32(0x0001, 1234);
    /// pv.put_u32(0x0002, 5678);
    /// ```
    pub fn new() -> Self {
        Self {
            data: BytesMut::new(),
        }
    }

    /// 从现有数据 `data` 创建一个打包的值表。
    ///
    /// # 示例
    ///
    /// ```
    /// use xtr::{PackedValues, Packet, PacketFlags};
    ///
    /// let mut pv = PackedValues::new();
    /// pv.put_u32(0x0001, 1234);
    /// pv.put_u32(0x0002, 5678);
    /// let packet = Packet::with_packed_values(pv, PacketFlags::empty(), 0);
    /// let pv = PackedValues::with_bytes(packet.as_ref());
    /// ```
    pub fn with_bytes(data: &[u8]) -> Self {
        Self {
            data: BytesMut::from(data),
        }
    }

    /// 获取打包的值表的数据的只读引用。
    pub fn as_bytes(&self) -> &[u8] {
        self.data.as_ref()
    }

    /// 获取打包的值表的数据的可变引用。
    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        self.data.as_mut()
    }

    /// 清除打包的值表的数据。
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// 当打包的值表的数据为空时返回 `true`。
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 返回打包的值表的条目的迭代器。
    ///
    /// # 示例
    ///
    /// ```
    /// use xtr::{PackedValues, PackedItemMapTo};
    ///
    /// let mut pv = PackedValues::new();
    /// pv.put_i32(0x0001, -1234);
    /// pv.put_u32(0x0002, 5678);
    ///
    /// for item in pv.items() {
    ///     println!("{:?}", item);
    ///     match item.addr {
    ///         0x0001 => {
    ///             let mut v = 0i32;
    ///             item.map_to(&pv, &mut v);
    ///             assert_eq!(v, -1234);
    ///         }
    ///         0x0002 => {
    ///             let mut v = 0u32;
    ///             item.map_to(&pv, &mut v);
    ///             assert_eq!(v, 5678);
    ///         }
    ///         _ => {}
    ///     }
    /// }
    /// ```
    pub fn items(&self) -> PackedItemIter<'_> {
        PackedItemIter::new(self.data.as_ref())
    }

    impl_op_typed!(i8, 0x81);
    impl_op_typed!(i16, 0x82);
    impl_op_typed!(i32, 0x84);
    impl_op_typed!(i64, 0x88);

    impl_op_typed!(u8, 0x01);
    impl_op_typed!(u16, 0x02);
    impl_op_typed!(u32, 0x04);
    impl_op_typed!(u64, 0x08);

    impl_op_typed!(f32, 0x44);
    impl_op_typed!(f64, 0x48);

    /// 获取指定地址 `addr` 类型为 `&str` 的值。
    pub fn get_str(&self, addr: u16) -> std::borrow::Cow<'_, str> {
        self.get_u8s(addr, 256)
            .map_or(std::borrow::Cow::from(""), |v| unsafe {
                std::borrow::Cow::Owned(String::from_utf8_unchecked(v))
            })
    }

    /// 设置指定地址 `addr` 类型为 `&str` 的值。
    pub fn put_str(&mut self, addr: u16, val: &str) {
        self.put_u8s(addr, val.as_bytes());
    }
}

impl Default for PackedValues {
    fn default() -> Self {
        Self::new()
    }
}

impl_read!(i8);
impl_read!(i16);
impl_read!(i32);
impl_read!(i64);

impl_read!(u8);
impl_read!(u16);
impl_read!(u32);
impl_read!(u64);

impl_read!(f32);
impl_read!(f64);

impl_write!(i8);
impl_write!(i16);
impl_write!(i32);
impl_write!(i64);

impl_write!(u8);
impl_write!(u16);
impl_write!(u32);
impl_write!(u64);

impl_write!(f32);
impl_write!(f64);

impl PackedValuesWrite<&str> for PackedValues {
    fn put(&mut self, addr: u16, val: &str) {
        self.put_u8s(addr, val.as_bytes());
    }

    fn put_many(&mut self, addr: u16, vals: &[&str]) {
        let joined = vals.join("");
        self.put_u8s(addr, joined.as_bytes());
    }
}

impl PackedValuesWrite<&[u8]> for PackedValues {
    fn put(&mut self, addr: u16, val: &[u8]) {
        self.put_u8s(addr, val);
    }

    fn put_many(&mut self, addr: u16, vals: &[&[u8]]) {
        let joined = vals.concat();
        self.put_u8s(addr, joined.as_slice());
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

impl From<PackedValueKind> for u8 {
    fn from(val: PackedValueKind) -> Self {
        match val {
            PackedValueKind::I8 => 0x81,
            PackedValueKind::I16 => 0x82,
            PackedValueKind::I32 => 0x84,
            PackedValueKind::I64 => 0x88,
            PackedValueKind::U8 => 0x01,
            PackedValueKind::U16 => 0x02,
            PackedValueKind::U32 => 0x04,
            PackedValueKind::U64 => 0x08,
            PackedValueKind::F32 => 0x44,
            PackedValueKind::F64 => 0x48,
            PackedValueKind::Unknown => 0,
        }
    }
}

/// 一个代表用于快速创建打包的值表的宏。
///
/// # 示例
///
/// ```
/// use xtr::pv;
///
/// let pv = pv![];
/// assert_eq!(pv.is_empty(), true);
///
/// let pv = pv![
///     (0x0000, 1234i16),
///     (0x0002, -1234i16),
///     (0x0004, 43210u16),
///     (0x0008, 12345678i32),
///     (0x000C, 12345.678f32),
///     (0x0010, 12345.678f64),
/// ];
#[macro_export]
macro_rules! pv {
    // pv![]
    [] => {
        $crate::PackedValues::new()
    };

    // pv![(reg, val), (reg,val), ...]
    [$(($reg:expr, $val:expr)),* $(,)?] => {
        {
            use $crate::PackedValuesWrite;
            let mut pv = $crate::PackedValues::new();
            $(
                pv.put($reg, $val);
            )*
            pv
        }
    };
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

    #[test]
    fn items_iter() {
        let mut pv = PackedValues::new();
        pv.put_i16(0x0000, 1234);
        pv.put_i16(0x0002, -1234);
        pv.put_u16(0x0004, 43210);
        pv.put_i32(0x0008, 12345678);
        pv.put_f32(0x000C, 12345.678);
        pv.put_f64(0x0010, 12345.678);
        pv.put_u8s(0x0018, &b"1234567"[..]);
        for item in pv.items() {
            match PackedValueKind::from(item.kind) {
                PackedValueKind::U8 => {
                    if item.elms > 1 {
                        let v = pv.peek_u8s(item.addr, item.elms as u16, item.ipos as usize);
                        assert!(v.is_some());
                        assert_eq!(v, Some(b"1234567".to_vec()));
                    }
                }
                PackedValueKind::I32 => {
                    let v = pv.peek_i32(item.addr, item.ipos as usize);
                    assert!(v.is_some());
                    assert_eq!(v, Some(12345678));
                }
                PackedValueKind::F32 => {
                    let v = pv.peek_f32(item.addr, item.ipos as usize);
                    assert!(v.is_some());
                    assert_eq!(v, Some(12345.678));
                }
                PackedValueKind::F64 => {
                    let v = pv.peek_f64(item.addr, item.ipos as usize);
                    assert!(v.is_some());
                    assert_eq!(v, Some(12345.678));
                }
                _ => {}
            }
        }
        for item in pv.items() {
            match item.addr {
                0x0000 => {
                    let mut v = 0i16;
                    item.map_to(&pv, &mut v);
                    assert_eq!(v, 1234);
                }
                0x0002 => {
                    let mut v = 0i16;
                    item.map_to(&pv, &mut v);
                    assert_eq!(v, -1234);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_pv() {
        let pv = pv![];
        assert_eq!(pv.is_empty(), true);

        let pv = pv![
            (0x0000, 1234i16),
            (0x0002, -1234i16),
            (0x0004, 43210u16),
            (0x0008, 12345678i32),
            (0x000C, 12345.678f32),
            (0x0010, 12345.678f64),
            (0x0020, "1234567"),
            (0x0030, &b"1234567"[..])
        ];
        assert_eq!(pv.get_i16(0x0000), Some(1234));
        assert_eq!(pv.get_i16(0x0002), Some(-1234));
        assert_eq!(pv.get_u16(0x0004), Some(43210));
        assert_eq!(pv.get_i32(0x0008), Some(12345678));
        assert_eq!(pv.get_f32(0x000C), Some(12345.678));
        assert_eq!(pv.get_f64(0x0010), Some(12345.678));
        assert_eq!(pv.get_u8s(0x0020, 7), Some(b"1234567".to_vec()));
        assert_eq!(pv.get_str(0x0020), "1234567");
        assert_eq!(pv.get_u8s(0x0030, 7), Some(b"1234567".to_vec()));
        assert_eq!(pv.get_str(0x0030), "1234567");
    }
}
