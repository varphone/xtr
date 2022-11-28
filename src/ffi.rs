use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::{
    Client, ClientEvent, ClientHandler, ClientState, PackedItem, PackedItemIter, PackedValueKind,
    PackedValues, Packet, PacketFlags, PacketHead, PacketType,
};
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Once};
use std::time::Duration;

pub type XtrClientState = ClientState;
pub type XtrClientPacketHandler = unsafe extern "C" fn(*mut XtrPacketRef, *mut c_void);
pub type XtrClientStateHandler = unsafe extern "C" fn(XtrClientState, *mut c_void);
pub type XtrPacketFlags = PacketFlags;
pub type XtrPacketRef = Arc<Packet>;
pub type XtrPacketType = PacketType;
pub type XtrPackedValuesRef = PackedValues;
pub type XtrPackedItemIterRef = PackedItemIter<'static>;
pub type XtrPackedItem = PackedItem;

static mut RT: Option<tokio::runtime::Runtime> = None;
static START_ENV_LOGGER: Once = Once::new();

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

enum ClientWrapperEvent {
    Start,
    Stop,
    Shutdown,
    SetPacketCB {
        cb: Option<XtrClientPacketHandler>,
        opaque: *mut c_void,
    },
    SetStateCB {
        cb: Option<XtrClientStateHandler>,
        opaque: *mut c_void,
    },
    ClientEvent(ClientEvent),
}

unsafe impl Send for ClientWrapperEvent {}

pub struct ClientWrapper {
    tx: Sender<ClientWrapperEvent>,
    _handle: JoinHandle<()>,
}

impl ClientWrapper {
    async fn run(addr: String, mut rx: Receiver<ClientWrapperEvent>) {
        let handler = Arc::new(MyHandler {
            on_packet: Callback::new(),
            on_state: Callback::new(),
        });
        let mut client = Client::new(addr, Arc::clone(&handler));
        loop {
            match rx.recv().await {
                Some(ev) => match ev {
                    ClientWrapperEvent::Start => {
                        if let Err(_err) = client.start().await {
                            break;
                        }
                    }
                    ClientWrapperEvent::Stop => {
                        if let Err(_err) = client.stop().await {
                            break;
                        }
                    }
                    ClientWrapperEvent::Shutdown => {
                        break;
                    }
                    ClientWrapperEvent::SetPacketCB { cb, opaque } => {
                        handler.on_packet.replace(cb, opaque);
                    }
                    ClientWrapperEvent::SetStateCB { cb, opaque } => {
                        handler.on_state.replace(cb, opaque);
                    }
                    ClientWrapperEvent::ClientEvent(ev) => {
                        client.send(ev);
                    }
                },
                None => {
                    break;
                }
            }
        }
    }

    pub fn new(addr: &str) -> Self {
        unsafe {
            let rt = RT.as_ref().unwrap();
            let addr = addr.to_string();
            let (tx, rx) = tokio::sync::mpsc::channel(16);
            let handle = rt.spawn(Self::run(addr, rx));
            Self {
                tx,
                _handle: handle,
            }
        }
    }

    pub fn set_packet_cb(&self, cb: Option<XtrClientPacketHandler>, opaque: *mut c_void) {
        let ev = ClientWrapperEvent::SetPacketCB { cb, opaque };
        let _r = self.tx.try_send(ev);
    }

    pub fn set_state_cb(&self, cb: Option<XtrClientStateHandler>, opaque: *mut c_void) {
        let ev = ClientWrapperEvent::SetStateCB { cb, opaque };
        let _r = self.tx.try_send(ev);
    }

    pub fn start(&self) -> i32 {
        self.tx
            .try_send(ClientWrapperEvent::Start)
            .map_or_else(|_| 0, |_| -1)
    }

    pub fn stop(&self) -> i32 {
        self.tx
            .try_send(ClientWrapperEvent::Stop)
            .map_or_else(|_| 0, |_| -1)
    }

    pub fn post(&self, packet: Arc<Packet>) -> i32 {
        let ev = ClientWrapperEvent::ClientEvent(ClientEvent::Packet(packet));
        self.tx.try_send(ev).map_or_else(|_| 0, |_| -1)
    }

    pub fn send(&self, packet: Arc<Packet>) -> Option<Arc<Packet>> {
        let ev = ClientWrapperEvent::ClientEvent(ClientEvent::Packet(packet));
        let _r = self.tx.try_send(ev);
        None
    }
}

impl Drop for ClientWrapper {
    fn drop(&mut self) {
        let _r = self.tx.try_send(ClientWrapperEvent::Shutdown);
    }
}

pub type XtrClient = Arc<ClientWrapper>;
pub type XtrClientPtr = *mut XtrClient;
pub type XtrClientConstPtr = *const XtrClient;

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrInitialize() {
    START_ENV_LOGGER.call_once(|| {
        use env_logger::Builder;

        let mut builder = Builder::from_default_env();

        builder.format_timestamp_millis().init();
    });

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
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
pub unsafe extern "C" fn XtrClientNew(addr: *const c_char, _flags: u32) -> XtrClientPtr {
    if addr.is_null() {
        return std::ptr::null_mut();
    }
    let addr = CStr::from_ptr(addr);
    let client = Arc::new(ClientWrapper::new(addr.to_str().unwrap()));
    Box::into_raw(Box::new(client))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientAddRef(xtr: XtrClientPtr) -> XtrClientPtr {
    let ctx = Arc::clone(&*xtr);
    Box::into_raw(Box::new(ctx))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientRelease(xtr: XtrClientPtr) {
    let _ = Box::from_raw(xtr);
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientPostPacket(xtr: XtrClientPtr, packet: *mut XtrPacketRef) -> i32 {
    let packet = Arc::clone(&*packet);
    (&*xtr).post(packet)
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientTryPostPacket(
    xtr: XtrClientPtr,
    packet: *mut XtrPacketRef,
) -> i32 {
    let packet = Arc::clone(&*packet);
    (&*xtr).post(packet)
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientSendPacket(
    xtr: XtrClientPtr,
    packet: *mut XtrPacketRef,
) -> *mut XtrPacketRef {
    let packet = Arc::clone(&*packet);
    (&*xtr)
        .send(packet)
        .map(|x| Box::into_raw(Box::new(x)))
        .unwrap_or(std::ptr::null_mut())
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientSetPacketCB(
    xtr: XtrClientPtr,
    cb: Option<XtrClientPacketHandler>,
    opaque: *mut c_void,
) {
    (&*xtr).set_packet_cb(cb, opaque)
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientSetStateCB(
    xtr: XtrClientPtr,
    cb: Option<XtrClientStateHandler>,
    opaque: *mut c_void,
) {
    (&*xtr).set_state_cb(cb, opaque)
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientStart(xtr: XtrClientPtr) -> i32 {
    (&*xtr).start()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientStop(xtr: XtrClientPtr) -> i32 {
    (&*xtr).stop()
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
pub unsafe extern "C" fn XtrPackedValuesGetI16s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *mut i16,
    num: u16,
) -> i32 {
    if let Some(v) = (&*pv).get_i16s(addr, num) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI32s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *mut i32,
    num: u16,
) -> i32 {
    if let Some(v) = (&*pv).get_i32s(addr, num) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI64s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *mut i64,
    num: u16,
) -> i32 {
    if let Some(v) = (&*pv).get_i64s(addr, num) {
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
        v.len() as i32
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU16s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *mut u16,
    num: u16,
) -> i32 {
    if let Some(v) = (&*pv).get_u16s(addr, num) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        v.len() as i32
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU32s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *mut u32,
    num: u16,
) -> i32 {
    if let Some(v) = (&*pv).get_u32s(addr, num) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        v.len() as i32
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU64s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *mut u64,
    num: u16,
) -> i32 {
    if let Some(v) = (&*pv).get_u64s(addr, num) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        v.len() as i32
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
pub unsafe extern "C" fn XtrPackedValuesGetF32s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *mut f32,
    num: u16,
) -> i32 {
    if let Some(v) = (&*pv).get_f32s(addr, num) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetF64s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *mut f64,
    num: u16,
) -> i32 {
    if let Some(v) = (&*pv).get_f64s(addr, num) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
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
pub unsafe extern "C" fn XtrPackedValuesPutI32(
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
pub unsafe extern "C" fn XtrPackedValuesPutI8s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *const i8,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (&mut *pv).put_i8s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI16s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *const i16,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (&mut *pv).put_i16s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI32s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *const i32,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (&mut *pv).put_i32s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI64s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *const i64,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (&mut *pv).put_i64s(addr, vals);
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
pub unsafe extern "C" fn XtrPackedValuesPutU8s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *const u8,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (&mut *pv).put_u8s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU16s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *const u16,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (&mut *pv).put_u16s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU32s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *const u32,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (&mut *pv).put_u32s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU64s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *const u64,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (&mut *pv).put_u64s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutF32(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: f32,
) -> i32 {
    (&mut *pv).put_f32(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutF64(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    val: f64,
) -> i32 {
    (&mut *pv).put_f64(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutF32s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *const f32,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (&mut *pv).put_f32s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutF64s(
    pv: *mut XtrPackedValuesRef,
    addr: u16,
    vals: *const f64,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (&mut *pv).put_f64s(addr, vals);
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
