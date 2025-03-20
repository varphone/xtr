//! 外部语言接口。
//!
//! 提供 C 风格的接口供外部语言使用，例如 C，C++，C#，Java，Python 等。
//!
use crate::{
    Client, ClientEvent, ClientHandler, ClientState, PackedItem, PackedItemIter, PackedValueKind,
    PackedValues, Packet, PacketFlags, PacketHead, PacketType,
};
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Once};
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

/// 一个代表客户端状态的枚举。
pub type XtrClientState = ClientState;

/// 一个代表客户端数据包回调的类型。
pub type XtrClientPacketHandler = unsafe extern "C" fn(XtrPacketPtr, *mut c_void);

/// 一个代表客户端状态回调的类型。
pub type XtrClientStateHandler = unsafe extern "C" fn(XtrClientState, *mut c_void);

/// 一个代表数据包智能指针的类型。
pub type XtrPacketRef = Arc<Packet>;

/// 一个代表数据包可写指针的类型。
pub type XtrPacketPtr = *mut XtrPacketRef;

/// 一个代表数据包只读指针的类型。
pub type XtrPacketConstPtr = *const XtrPacketRef;

/// 一个代表打包的值表的可写指针的类型。
pub type XtrPackedValuesPtr = *mut PackedValues;

/// 一个代表打包的值表的只读指针的类型。
pub type XtrPackedValuesConstPtr = *const PackedValues;

/// 一个代表打包的值表的迭代器的可写指针的类型。
pub type XtrPackedItemIterPtr = *mut PackedItemIter<'static>;

/// 一个代表打包的值表的迭代器的只读指针的类型。
pub type XtrPackedItemIterConstPtr = *const PackedItemIter<'static>;

static RT: AtomicPtr<tokio::runtime::Runtime> = AtomicPtr::new(std::ptr::null_mut());
static START_ENV_LOGGER: Once = Once::new();

fn get_rt<'a>() -> Option<&'a tokio::runtime::Runtime> {
    unsafe { RT.load(Ordering::SeqCst).as_ref() }
}

enum ForwardEvent {
    Shutdown,
    Packet(Arc<Packet>),
    PacketCallback(Option<XtrClientPacketHandler>, *mut c_void),
    State(ClientState),
    StateCallback(Option<XtrClientStateHandler>, *mut c_void),
}

unsafe impl Send for ForwardEvent {}
unsafe impl Sync for ForwardEvent {}

struct MyHandler {
    forward_tx: Sender<ForwardEvent>,
}

impl ClientHandler for MyHandler {
    fn on_packet(&self, packet: Arc<Packet>) {
        let _ = self.forward_tx.try_send(ForwardEvent::Packet(packet));
    }

    fn on_state(&self, state: ClientState) {
        let _ = self.forward_tx.try_send(ForwardEvent::State(state));
    }
}

enum XtrClientEvent {
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

unsafe impl Send for XtrClientEvent {}

/// 一个代表 XTR 客户端的类型。
pub struct XtrClient {
    tx: Sender<XtrClientEvent>,
    tsk_thread: Option<JoinHandle<()>>,
    fwd_thread: Option<JoinHandle<()>>,
}

impl XtrClient {
    async fn run(addr: String, mut rx: Receiver<XtrClientEvent>, forward_tx: Sender<ForwardEvent>) {
        let handler = Arc::new(MyHandler {
            forward_tx: forward_tx.clone(),
        });
        let mut client = Client::new(addr, Arc::clone(&handler));
        while let Some(ev) = rx.recv().await {
            match ev {
                XtrClientEvent::Start => {
                    if let Err(_err) = client.start().await {
                        break;
                    }
                }
                XtrClientEvent::Stop => {
                    if let Err(_err) = client.stop().await {
                        break;
                    }
                }
                XtrClientEvent::Shutdown => {
                    break;
                }
                XtrClientEvent::SetPacketCB { cb, opaque } => {
                    let _ = forward_tx
                        .send(ForwardEvent::PacketCallback(cb, opaque))
                        .await;
                }
                XtrClientEvent::SetStateCB { cb, opaque } => {
                    let _ = forward_tx
                        .send(ForwardEvent::StateCallback(cb, opaque))
                        .await;
                }
                XtrClientEvent::ClientEvent(ev) => {
                    client.send(ev);
                }
            }
        }
        let _ = forward_tx.send(ForwardEvent::Shutdown).await;
    }

    fn forawd(mut rx: Receiver<ForwardEvent>) {
        unsafe {
            let mut packet_cb: Option<XtrClientPacketHandler> = None;
            let mut packet_cb_opaque: *mut c_void = std::ptr::null_mut();
            let mut state_cb: Option<XtrClientStateHandler> = None;
            let mut state_cb_opaque: *mut c_void = std::ptr::null_mut();
            while let Some(ev) = rx.blocking_recv() {
                match ev {
                    ForwardEvent::Shutdown => {
                        break;
                    }
                    ForwardEvent::PacketCallback(cb, opaque) => {
                        packet_cb = cb;
                        packet_cb_opaque = opaque;
                    }
                    ForwardEvent::StateCallback(cb, opaque) => {
                        state_cb = cb;
                        state_cb_opaque = opaque;
                    }
                    ForwardEvent::Packet(packet) => {
                        if let Some(cb) = packet_cb {
                            cb(Box::into_raw(Box::new(packet)), packet_cb_opaque);
                        }
                    }
                    ForwardEvent::State(state) => {
                        if let Some(cb) = state_cb {
                            cb(state, state_cb_opaque);
                        }
                    }
                }
            }
        }
    }

    pub fn new(addr: &str) -> Self {
        let rt = get_rt().unwrap();
        let addr = addr.to_string();
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let (forward_tx, forward_rx) = tokio::sync::mpsc::channel(100);
        let tsk_thread = rt.spawn(Self::run(addr, rx, forward_tx.clone()));
        let fwd_thread = rt.spawn_blocking(move || Self::forawd(forward_rx));
        Self {
            tx,
            tsk_thread: Some(tsk_thread),
            fwd_thread: Some(fwd_thread),
        }
    }

    pub fn set_packet_cb(&self, cb: Option<XtrClientPacketHandler>, opaque: *mut c_void) {
        let ev = XtrClientEvent::SetPacketCB { cb, opaque };
        let _r = self.tx.try_send(ev);
    }

    pub fn set_state_cb(&self, cb: Option<XtrClientStateHandler>, opaque: *mut c_void) {
        let ev = XtrClientEvent::SetStateCB { cb, opaque };
        let _r = self.tx.try_send(ev);
    }

    pub fn start(&self) -> i32 {
        self.tx
            .try_send(XtrClientEvent::Start)
            .map_or_else(|_| 0, |_| -1)
    }

    pub fn stop(&self) -> i32 {
        self.tx
            .try_send(XtrClientEvent::Stop)
            .map_or_else(|_| 0, |_| -1)
    }

    pub fn post(&self, packet: Arc<Packet>) -> i32 {
        let ev = XtrClientEvent::ClientEvent(ClientEvent::Packet(packet));
        self.tx.try_send(ev).map_or_else(|_| 0, |_| -1)
    }

    pub fn send(&self, packet: Arc<Packet>) -> Option<Arc<Packet>> {
        let ev = XtrClientEvent::ClientEvent(ClientEvent::Packet(packet));
        let _r = self.tx.try_send(ev);
        None
    }
}

impl Drop for XtrClient {
    fn drop(&mut self) {
        if let Some(rt) = get_rt() {
            rt.block_on(async {
                let _r = self.tx.send(XtrClientEvent::Shutdown).await;
                if let Some(tsk_thread) = self.tsk_thread.take() {
                    let _r = tsk_thread.await;
                }
                if let Some(fwd_thread) = self.fwd_thread.take() {
                    let _r = fwd_thread.await;
                }
            });
        }
    }
}

/// 一个代表 XTR 客户端实例智能指针的类型。
pub type XtrClientRef = Arc<XtrClient>;

/// 一个代表 XTR 客户端实例可写指针的类型。
pub type XtrClientPtr = *mut XtrClientRef;

/// 一个代表 XTR 客户端实例只读指针的类型。
pub type XtrClientConstPtr = *const XtrClientRef;

/// 初始化 XTR 框架。
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
    let old = RT.swap(Box::into_raw(Box::new(rt)), Ordering::SeqCst);
    if !old.is_null() {
        let _ = Box::from_raw(old);
    }
}

/// 释放 XTR 框架。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrFinalize() {
    let rt = RT.swap(std::ptr::null_mut(), Ordering::SeqCst);
    if !rt.is_null() {
        Box::from_raw(rt).shutdown_timeout(Duration::from_millis(200));
    }
}

/// 创建客户端实例。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientNew(addr: *const c_char, _flags: u32) -> XtrClientPtr {
    if addr.is_null() {
        return std::ptr::null_mut();
    }
    let addr = CStr::from_ptr(addr);
    let client = Arc::new(XtrClient::new(addr.to_str().unwrap()));
    Box::into_raw(Box::new(client))
}

/// 增加客户端实例引用计数。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientAddRef(xtr: XtrClientPtr) -> XtrClientPtr {
    let ctx = Arc::clone(&*xtr);
    Box::into_raw(Box::new(ctx))
}

/// 减少客户端实例引用计数，当减到 0 时会释放资源。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientRelease(xtr: XtrClientPtr) {
    let _ = Box::from_raw(xtr);
}

/// 向客户端连接推入一个数据包并立即返回。
///
/// 推入的数据包会存放在队列中由后台按顺序发送到连接的远端。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientPostPacket(xtr: XtrClientPtr, packet: XtrPacketPtr) -> i32 {
    let packet = Arc::clone(&*packet);
    (*xtr).post(packet)
}

/// 尝试向客户端连接推入一个数据包并立即返回。
///
/// 推入的数据包会存放在队列中由后台按顺序发送到连接的远端。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientTryPostPacket(xtr: XtrClientPtr, packet: XtrPacketPtr) -> i32 {
    let packet = Arc::clone(&*packet);
    (*xtr).post(packet)
}

/// 向客户端连接发送一个数据包并等待返回。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientSendPacket(
    xtr: XtrClientPtr,
    packet: XtrPacketPtr,
) -> XtrPacketPtr {
    let packet = Arc::clone(&*packet);
    (*xtr)
        .send(packet)
        .map(|x| Box::into_raw(Box::new(x)))
        .unwrap_or(std::ptr::null_mut())
}

/// 设置客户端实例数据包回调。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientSetPacketCB(
    xtr: XtrClientPtr,
    cb: XtrClientPacketHandler,
    opaque: *mut c_void,
) {
    (*xtr).set_packet_cb(Some(cb), opaque)
}

/// 设置客户端实例状态回调。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientSetStateCB(
    xtr: XtrClientPtr,
    cb: XtrClientStateHandler,
    opaque: *mut c_void,
) {
    (*xtr).set_state_cb(Some(cb), opaque)
}

/// 启动客户端实例。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientStart(xtr: XtrClientPtr) -> i32 {
    (*xtr).start()
}

/// 停止客户端实例。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrClientStop(xtr: XtrClientPtr) -> i32 {
    (*xtr).stop()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketNewData(length: u32, flags: u8, stream_id: u32) -> XtrPacketPtr {
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
    pv: XtrPackedValuesConstPtr,
    flags: u8,
    stream_id: u32,
) -> XtrPacketPtr {
    let bytes = (*pv).as_bytes();
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
pub unsafe extern "C" fn XtrPacketAddRef(packet: XtrPacketPtr) -> XtrPacketPtr {
    if packet.is_null() {
        return std::ptr::null_mut();
    }
    let refer = Arc::clone(&*packet);
    Box::into_raw(Box::new(refer))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketRelease(packet: XtrPacketPtr) {
    if !packet.is_null() {
        let _ = Box::from_raw(packet);
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetFlags(packet: XtrPacketConstPtr) -> u8 {
    (*packet).flags().bits()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetLength(packet: XtrPacketConstPtr) -> u32 {
    (*packet).length()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetSequence(packet: XtrPacketConstPtr) -> u32 {
    (*packet).seq()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetStreamId(packet: XtrPacketConstPtr) -> u32 {
    (*packet).stream_id()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetTimestamp(packet: XtrPacketConstPtr) -> u64 {
    (*packet).ts()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetType(packet: XtrPacketConstPtr) -> u8 {
    (*packet).type_() as u8
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetConstData(packet: XtrPacketConstPtr) -> *const u8 {
    (*packet).data.as_ptr()
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPacketGetData(packet: XtrPacketConstPtr) -> *mut u8 {
    // FIXME:
    (*packet).data.as_ptr() as *mut u8
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesNew() -> XtrPackedValuesPtr {
    let pv = PackedValues::new();
    Box::into_raw(Box::new(pv))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesWithBytes(
    data: *const u8,
    length: u32,
) -> XtrPackedValuesPtr {
    let bytes = std::slice::from_raw_parts(data, length as usize);
    let pv = PackedValues::with_bytes(bytes);
    Box::into_raw(Box::new(pv))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesRelease(pv: XtrPackedValuesPtr) {
    let _ = Box::from_raw(pv);
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI8(
    pv: XtrPackedValuesPtr,
    addr: u16,
    val: *mut i8,
) -> i32 {
    if let Some(v) = (*pv).get_i8(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI16(
    pv: XtrPackedValuesPtr,
    addr: u16,
    val: *mut i16,
) -> i32 {
    if let Some(v) = (*pv).get_i16(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI32(
    pv: XtrPackedValuesPtr,
    addr: u16,
    val: *mut i32,
) -> i32 {
    if let Some(v) = (*pv).get_i32(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI64(
    pv: XtrPackedValuesPtr,
    addr: u16,
    val: *mut i64,
) -> i32 {
    if let Some(v) = (*pv).get_i64(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetI8s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *mut i8,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).get_i8s(addr, num) {
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
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *mut i16,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).get_i16s(addr, num) {
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
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *mut i32,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).get_i32s(addr, num) {
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
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *mut i64,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).get_i64s(addr, num) {
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
    pv: XtrPackedValuesPtr,
    addr: u16,
    val: *mut u8,
) -> i32 {
    if let Some(v) = (*pv).get_u8(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU16(
    pv: XtrPackedValuesPtr,
    addr: u16,
    val: *mut u16,
) -> i32 {
    if let Some(v) = (*pv).get_u16(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU32(
    pv: XtrPackedValuesPtr,
    addr: u16,
    val: *mut u32,
) -> i32 {
    if let Some(v) = (*pv).get_u32(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU64(
    pv: XtrPackedValuesPtr,
    addr: u16,
    val: *mut u64,
) -> i32 {
    if let Some(v) = (*pv).get_u64(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetU8s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *mut u8,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).get_u8s(addr, num) {
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
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *mut u16,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).get_u16s(addr, num) {
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
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *mut u32,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).get_u32s(addr, num) {
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
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *mut u64,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).get_u64s(addr, num) {
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
    pv: XtrPackedValuesPtr,
    addr: u16,
    val: *mut f32,
) -> i32 {
    if let Some(v) = (*pv).get_f32(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetF64(
    pv: XtrPackedValuesPtr,
    addr: u16,
    val: *mut f64,
) -> i32 {
    if let Some(v) = (*pv).get_f64(addr) {
        *val = v;
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesGetF32s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *mut f32,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).get_f32s(addr, num) {
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
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *mut f64,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).get_f64s(addr, num) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}

// macro_rules! wrap_peek_x {
//     ($i:ident, $t:ty) => {
//         paste::paste! {
//             #[doc = " 获取指定偏移 `ipos` 处地址 `addr` 类型为 `" $t "` 的值。"]
//             #[doc = " # Safety"]
//             #[no_mangle]
//             pub unsafe extern "C" fn [<$i>](pv: XtrPackedValuesPtr, addr: u16, ipos: u64, val: *mut $t) -> i32 {
//                 if let Some(v) = (*pv).[<peek_ $t>](addr, ipos as usize) {
//                     *val = v;
//                     0
//                 } else {
//                     -1
//                 }
//             }
//         }
//     }
// }

// macro_rules! wrap_peek_xs {
//     ($i:ident, $t:ty) => {
//         paste::paste! {
//             #[doc = " 获取指定偏移 `ipos` 处地址 `addr` 类型为 `" $t "` 的多个值。"]
//             #[doc = " # Safety"]
//             #[no_mangle]
//             pub unsafe extern "C" fn [<$i>](pv: XtrPackedValuesPtr, addr: u16, ipos: u64, vals: *mut $t, num: u16) -> i32 {
//                 if let Some(v) = (*pv).[<peek_ $t s>](addr, num, ipos as usize) {
//                     let vals = std::slice::from_raw_parts_mut(vals, num as usize);
//                     vals[0..v.len()].copy_from_slice(&v[..]);
//                     0
//                 } else {
//                     -1
//                 }
//             }
//         }
//     }
// }

// wrap_peek_x!(XtrPackedValuesPeekI8, i8);
// wrap_peek_x!(XtrPackedValuesPeekI16, i32);
// wrap_peek_x!(XtrPackedValuesPeekI32, i32);
// wrap_peek_x!(XtrPackedValuesPeekI64, i64);
// wrap_peek_x!(XtrPackedValuesPeekU8, u8);
// wrap_peek_x!(XtrPackedValuesPeekU16, u32);
// wrap_peek_x!(XtrPackedValuesPeekU32, u32);
// wrap_peek_x!(XtrPackedValuesPeekU64, u64);
// wrap_peek_x!(XtrPackedValuesPeekF32, f32);
// wrap_peek_x!(XtrPackedValuesPeekF64, f64);

// wrap_peek_xs!(XtrPackedValuesPeekI8s, i8);
// wrap_peek_xs!(XtrPackedValuesPeekI16s, i32);
// wrap_peek_xs!(XtrPackedValuesPeekI32s, i32);
// wrap_peek_xs!(XtrPackedValuesPeekI64s, i64);
// wrap_peek_xs!(XtrPackedValuesPeekU8s, u8);
// wrap_peek_xs!(XtrPackedValuesPeekU16s, u32);
// wrap_peek_xs!(XtrPackedValuesPeekU32s, u32);
// wrap_peek_xs!(XtrPackedValuesPeekU64s, u64);
// wrap_peek_xs!(XtrPackedValuesPeekF32s, f32);
// wrap_peek_xs!(XtrPackedValuesPeekF64s, f64);

/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `i8` 的值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekI8(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    val: *mut i8,
) -> i32 {
    if let Some(v) = (*pv).peek_i8(addr, ipos as usize) {
        *val = v;
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `i32` 的值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekI16(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    val: *mut i32,
) -> i32 {
    if let Some(v) = (*pv).peek_i32(addr, ipos as usize) {
        *val = v;
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `i32` 的值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekI32(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    val: *mut i32,
) -> i32 {
    if let Some(v) = (*pv).peek_i32(addr, ipos as usize) {
        *val = v;
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `i64` 的值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekI64(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    val: *mut i64,
) -> i32 {
    if let Some(v) = (*pv).peek_i64(addr, ipos as usize) {
        *val = v;
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `u8` 的值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekU8(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    val: *mut u8,
) -> i32 {
    if let Some(v) = (*pv).peek_u8(addr, ipos as usize) {
        *val = v;
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `u32` 的值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekU16(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    val: *mut u32,
) -> i32 {
    if let Some(v) = (*pv).peek_u32(addr, ipos as usize) {
        *val = v;
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `u32` 的值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekU32(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    val: *mut u32,
) -> i32 {
    if let Some(v) = (*pv).peek_u32(addr, ipos as usize) {
        *val = v;
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `u64` 的值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekU64(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    val: *mut u64,
) -> i32 {
    if let Some(v) = (*pv).peek_u64(addr, ipos as usize) {
        *val = v;
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `f32` 的值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekF32(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    val: *mut f32,
) -> i32 {
    if let Some(v) = (*pv).peek_f32(addr, ipos as usize) {
        *val = v;
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `f64` 的值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekF64(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    val: *mut f64,
) -> i32 {
    if let Some(v) = (*pv).peek_f64(addr, ipos as usize) {
        *val = v;
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `i8` 的多个值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekI8s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    vals: *mut i8,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).peek_i8s(addr, num, ipos as usize) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `i32` 的多个值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekI16s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    vals: *mut i32,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).peek_i32s(addr, num, ipos as usize) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `i32` 的多个值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekI32s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    vals: *mut i32,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).peek_i32s(addr, num, ipos as usize) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `i64` 的多个值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekI64s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    vals: *mut i64,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).peek_i64s(addr, num, ipos as usize) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `u8` 的多个值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekU8s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    vals: *mut u8,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).peek_u8s(addr, num, ipos as usize) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `u32` 的多个值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekU16s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    vals: *mut u32,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).peek_u32s(addr, num, ipos as usize) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `u32` 的多个值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekU32s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    vals: *mut u32,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).peek_u32s(addr, num, ipos as usize) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `u64` 的多个值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekU64s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    vals: *mut u64,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).peek_u64s(addr, num, ipos as usize) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `f32` 的多个值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekF32s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    vals: *mut f32,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).peek_f32s(addr, num, ipos as usize) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}
/// 获取指定偏移 `ipos` 处地址 `addr` 类型为 `f64` 的多个值。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPeekF64s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    ipos: u64,
    vals: *mut f64,
    num: u16,
) -> i32 {
    if let Some(v) = (*pv).peek_f64s(addr, num, ipos as usize) {
        let vals = std::slice::from_raw_parts_mut(vals, num as usize);
        vals[0..v.len()].copy_from_slice(&v[..]);
        0
    } else {
        -1
    }
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI8(pv: XtrPackedValuesPtr, addr: u16, val: i8) -> i32 {
    (*pv).put_i8(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI16(pv: XtrPackedValuesPtr, addr: u16, val: i16) -> i32 {
    (*pv).put_i16(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI32(pv: XtrPackedValuesPtr, addr: u16, val: i32) -> i32 {
    (*pv).put_i32(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI64(pv: XtrPackedValuesPtr, addr: u16, val: i64) -> i32 {
    (*pv).put_i64(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI8s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *const i8,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (*pv).put_i8s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI16s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *const i16,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (*pv).put_i16s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI32s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *const i32,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (*pv).put_i32s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutI64s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *const i64,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (*pv).put_i64s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU8(pv: XtrPackedValuesPtr, addr: u16, val: u8) -> i32 {
    (*pv).put_u8(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU16(pv: XtrPackedValuesPtr, addr: u16, val: u16) -> i32 {
    (*pv).put_u16(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU32(pv: XtrPackedValuesPtr, addr: u16, val: u32) -> i32 {
    (*pv).put_u32(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU64(pv: XtrPackedValuesPtr, addr: u16, val: u64) -> i32 {
    (*pv).put_u64(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU8s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *const u8,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (*pv).put_u8s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU16s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *const u16,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (*pv).put_u16s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU32s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *const u32,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (*pv).put_u32s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutU64s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *const u64,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (*pv).put_u64s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutF32(pv: XtrPackedValuesPtr, addr: u16, val: f32) -> i32 {
    (*pv).put_f32(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutF64(pv: XtrPackedValuesPtr, addr: u16, val: f64) -> i32 {
    (*pv).put_f64(addr, val);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutF32s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *const f32,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (*pv).put_f32s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesPutF64s(
    pv: XtrPackedValuesPtr,
    addr: u16,
    vals: *const f64,
    num: u16,
) -> i32 {
    let vals = std::slice::from_raw_parts(vals, num as usize);
    (*pv).put_f64s(addr, vals);
    0
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesItemIter(pv: XtrPackedValuesPtr) -> XtrPackedItemIterPtr {
    Box::into_raw(Box::new((*pv).items()))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesItemIterRelease(iter: XtrPackedItemIterPtr) {
    let _ = Box::from_raw(iter);
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrPackedValuesItemNext(iter: XtrPackedItemIterPtr) -> PackedItem {
    (*iter).next().unwrap_or(PackedItem {
        addr: 0,
        kind: PackedValueKind::Unknown.into(),
        elms: 0,
        ipos: 0,
    })
}

/// 返回微秒精度的当前世界时钟时间戳。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrRealTimeTsNow() -> u64 {
    crate::Timestamp::now_realtime().as_micros()
}

/// 返回微秒精度的当前恒增时钟时间戳。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn XtrMonotonicTsNow() -> u64 {
    crate::Timestamp::now_monotonic().as_micros()
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
