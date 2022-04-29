use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use xtr::{
    Client, ClientEvent, ClientHandler, ClientState, PackedItem, PackedItemIter, PackedValueKind,
    PackedValues, Packet, PacketFlags, PacketHead, PacketType,
};

pub type XtrClientState = ClientState;
pub type XtrClientPacketHandler = unsafe extern "C" fn(*mut XtrPacketRef, *mut c_void);
pub type XtrClientStateHandler = unsafe extern "C" fn(XtrClientState, *mut c_void);
pub type XtrPacketFlags = PacketFlags;
pub type XtrPacketRef = Arc<Packet>;
pub type XtrPacketType = PacketType;
pub type XtrPackedValuesRef = PackedValues;
pub type XtrPackedItemIterRef = PackedItemIter<'static>;
pub type XtrPackedItem = PackedItem;

struct Callback<T> {
    cb: AtomicPtr<Option<T>>,
    opaque: AtomicPtr<c_void>,
}

impl<T> Callback<T> {
    pub fn new() -> Self {
        Self {
            cb: AtomicPtr::new(Box::into_raw(Box::new(None))),
            opaque: Default::default(),
        }
    }

    pub fn replace(&self, cb: Option<T>, opaque: *mut c_void) {
        unsafe {
            let ptr = self.cb.load(Ordering::SeqCst);
            let _ = Box::from_raw(ptr);
            let ptr = Box::into_raw(Box::new(cb));
            self.cb.store(ptr, Ordering::SeqCst);
            self.opaque.store(opaque, Ordering::SeqCst);
        }
    }
}

struct MyHandler {
    on_packet: Callback<XtrClientPacketHandler>,
    on_state: Callback<XtrClientStateHandler>,
}

impl ClientHandler for MyHandler {
    fn on_packet(&self, packet: Arc<Packet>) {
        unsafe {
            let cb = self.on_packet.cb.load(Ordering::SeqCst);
            let opaque = self.on_packet.opaque.load(Ordering::SeqCst);
            match &mut *cb {
                Some(cb) => {
                    cb(Box::into_raw(Box::new(packet)), opaque);
                }
                None => {}
            }
        }
    }

    fn on_state(&self, state: ClientState) {
        unsafe {
            let cb = self.on_state.cb.load(Ordering::SeqCst);
            let opaque = self.on_state.opaque.load(Ordering::SeqCst);
            match &mut *cb {
                Some(cb) => {
                    cb(state, opaque);
                }
                None => {}
            }
        }
    }
}

pub struct ClientCtx {
    handler: Arc<MyHandler>,
    client: Client,
}

impl ClientCtx {
    pub fn set_packet_cb(&mut self, cb: Option<XtrClientPacketHandler>, opaque: *mut c_void) {
        self.handler.on_packet.replace(cb, opaque);
    }

    pub fn set_state_cb(&mut self, cb: Option<XtrClientStateHandler>, opaque: *mut c_void) {
        self.handler.on_state.replace(cb, opaque);
    }

    pub async fn start(&mut self) -> i32 {
        self.client
            .start()
            .await
            .map(|_| 0)
            .map_err(|_| -1)
            .unwrap()
    }

    pub async fn stop(&mut self) -> i32 {
        self.client.stop().await.map(|_| 0).map_err(|_| -1).unwrap()
    }

    pub fn post(&mut self, packet: Arc<Packet>) -> i32 {
        self.client.send(ClientEvent::Packet(packet));
        0
    }

    pub fn send(&mut self, packet: Arc<Packet>) -> Option<Arc<Packet>> {
        self.client.send(ClientEvent::Packet(packet));
        None
    }
}

pub type XtrClientRef = Arc<Mutex<ClientCtx>>;

static mut RT: Option<tokio::runtime::Runtime> = None;

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrInitialize() {
    use env_logger::Builder;

    let mut builder = Builder::from_default_env();

    builder.init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    RT = Some(rt);
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrFinalize() {
    if let Some(rt) = RT.take() {
        rt.shutdown_timeout(Duration::from_millis(200));
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientNew(addr: *const c_char, _flags: u32) -> *mut XtrClientRef {
    if addr.is_null() {
        return std::ptr::null_mut();
    }
    let handler = Arc::new(MyHandler {
        on_packet: Callback::new(),
        on_state: Callback::new(),
    });
    let addr = CStr::from_ptr(addr);
    let client = Client::new(addr.to_str().unwrap(), Arc::clone(&handler));
    let ctx = Arc::new(Mutex::new(ClientCtx { handler, client }));
    Box::into_raw(Box::new(ctx))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientAddRef(xtr: *mut XtrClientRef) -> *mut XtrClientRef {
    let ctx = Arc::clone(&*xtr);
    Box::into_raw(Box::new(ctx))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientRelease(xtr: *mut XtrClientRef) {
    let _ = Box::from_raw(xtr);
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientPostPacket(
    xtr: *mut XtrClientRef,
    packet: *mut XtrPacketRef,
) -> i32 {
    let packet = Arc::clone(&*packet);
    (&*xtr).lock().unwrap().post(packet)
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientSendPacket(
    xtr: *mut XtrClientRef,
    packet: *mut XtrPacketRef,
) -> *mut XtrPacketRef {
    let packet = Arc::clone(&*packet);
    (&*xtr)
        .lock()
        .unwrap()
        .send(packet)
        .map(|x| Box::into_raw(Box::new(x)))
        .unwrap_or(std::ptr::null_mut())
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientSetPacketCB(
    xtr: *mut XtrClientRef,
    cb: Option<XtrClientPacketHandler>,
    opaque: *mut c_void,
) {
    (&*xtr).lock().unwrap().set_packet_cb(cb, opaque)
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientSetStateCB(
    xtr: *mut XtrClientRef,
    cb: Option<XtrClientStateHandler>,
    opaque: *mut c_void,
) {
    (&*xtr).lock().unwrap().set_state_cb(cb, opaque)
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientStart(xtr: *mut XtrClientRef) -> i32 {
    if let Some(ref rt) = RT {
        rt.block_on((&*xtr).lock().unwrap().start())
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientStop(xtr: *mut XtrClientRef) -> i32 {
    if let Some(ref rt) = RT {
        rt.block_on((&*xtr).lock().unwrap().stop())
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketNewData(
    length: u32,
    flags: u8,
    stream_id: u32,
) -> *mut XtrPacketRef {
    let head = PacketHead::new(
        length,
        PacketType::Data,
        PacketFlags::from(flags),
        stream_id,
    );
    let ptr = Box::new(Arc::new(Packet::alloc_data(head)));
    Box::into_raw(ptr)
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketNewPackedValues(
    pv: *const XtrPackedValuesRef,
    flags: u8,
    stream_id: u32,
) -> *mut XtrPacketRef {
    let bytes = (&*pv).as_bytes();
    let head = PacketHead::new(
        bytes.len() as u32,
        PacketType::PackedValues,
        PacketFlags::from(flags),
        stream_id,
    );
    let ptr = Box::new(Arc::new(Packet::with_data(head, bytes)));
    Box::into_raw(ptr)
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketAddRef(packet: *mut XtrPacketRef) -> *mut XtrPacketRef {
    if packet.is_null() {
        return std::ptr::null_mut();
    }
    let refer = Arc::clone(&*packet);
    Box::into_raw(Box::new(refer))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketRelease(packet: *mut XtrPacketRef) {
    if !packet.is_null() {
        let _ = Box::from_raw(packet);
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetFlags(packet: *const XtrPacketRef) -> u8 {
    (&*packet).flags().bits()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetLength(packet: *const XtrPacketRef) -> u32 {
    (&*packet).length()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetSequence(packet: *const XtrPacketRef) -> u32 {
    (&*packet).seq() as u32
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetStreamId(packet: *const XtrPacketRef) -> u32 {
    (&*packet).stream_id()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetTimestamp(packet: *const XtrPacketRef) -> u64 {
    (&*packet).ts()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetType(packet: *const XtrPacketRef) -> u8 {
    (&*packet).type_() as u8
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetConstData(packet: *const XtrPacketRef) -> *const u8 {
    (*packet).data.as_ptr()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetData(packet: *const XtrPacketRef) -> *mut u8 {
    // FIXME:
    (*packet).data.as_ptr() as *mut u8
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesNew() -> *mut XtrPackedValuesRef {
    let pv = PackedValues::new();
    Box::into_raw(Box::new(pv))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesWithBytes(
    data: *const u8,
    length: u32,
) -> *mut XtrPackedValuesRef {
    let bytes = std::slice::from_raw_parts(data, length as usize);
    let pv = PackedValues::with_bytes(bytes);
    Box::into_raw(Box::new(pv))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesRelease(pv: *mut XtrPackedValuesRef) {
    let _ = Box::from_raw(pv);
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI8(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: *mut i8,
) -> i32 {
    if let Some(v) = (&*pv).get_i8(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI16(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: *mut i16,
) -> i32 {
    if let Some(v) = (&*pv).get_i16(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI32(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: *mut i32,
) -> i32 {
    if let Some(v) = (&*pv).get_i32(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI64(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: *mut i64,
) -> i32 {
    if let Some(v) = (&*pv).get_i64(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI8s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *mut i8,
    num: u16,
) -> i32 {
    if let Some(v) = (&*pv).get_i8s(addr, num) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU8(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: *mut u8,
) -> i32 {
    if let Some(v) = (&*pv).get_u8(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU16(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: *mut u16,
) -> i32 {
    if let Some(v) = (&*pv).get_u16(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU32(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: *mut u32,
) -> i32 {
    if let Some(v) = (&*pv).get_u32(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU64(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: *mut u64,
) -> i32 {
    if let Some(v) = (&*pv).get_u64(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU8s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *mut u8,
    num: u16,
) -> i32 {
    if let Some(v) = (&*pv).get_u8s(addr, num) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetF32(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: *mut f32,
) -> i32 {
    if let Some(v) = (&*pv).get_f32(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetF64(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: *mut f64,
) -> i32 {
    if let Some(v) = (&*pv).get_f64(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI8(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: i8,
) -> i32 {
    (&mut *pv).put_i8(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI16(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: i16,
) -> i32 {
    (&mut *pv).put_i16(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI132(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: i32,
) -> i32 {
    (&mut *pv).put_i32(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI64(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: i64,
) -> i32 {
    (&mut *pv).put_i64(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU8(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: u8,
) -> i32 {
    (&mut *pv).put_u8(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU16(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: u16,
) -> i32 {
    (&mut *pv).put_u16(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU32(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: u32,
) -> i32 {
    (&mut *pv).put_u32(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU64(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: u64,
) -> i32 {
    (&mut *pv).put_u64(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesItemIter(
    pv: *mut XtrPackedValuesRef,
) -> *mut XtrPackedItemIterRef {
    Box::into_raw(Box::new((&*pv).items()))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesItemIterRelease(iter: *mut XtrPackedItemIterRef) {
    let _ = Box::from_raw(iter);
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesItemNext(iter: *mut XtrPackedItemIterRef) -> XtrPackedItem {
    (&mut *iter).next().unwrap_or(PackedItem {
        addr: 0,
        kind: PackedValueKind::Unknown,
        elms: 0,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
