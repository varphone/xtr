use std::ffi::raw::{c_char, c_void};
use std::sync::Arc;
use xtr::{ClientHandler, Packet, PacketFlags, PacketHead, PacketType};

pub type XtrClientState = ClientState;
pub type XtrClientPacketHandler = unsafe extern "C" fn(*mut XtrPacketRef, *mut c_void);
pub type XtrClientStateHandler = unsafe extern "C" fn(XtrClientState, *mut c_void);
pub type XtrPacketFlags = PacketFlags;
pub type XtrPacketRef = Arc<Packet>;
pub type XtrPacketType = PacketType;

struct Callback<T> {
    cb: AtomicPtr<T>,
    opaque: AtomicPtr<c_void>,
}

struct MyHandler {
    on_packet: Callback<XtrClientPacketHandler>,
    on_state: Callback<XtrClientStateHandler>,
}

impl ClientHandler for MyHandler {
    fn on_packet(&self, packet: Arc<Packet>) {
        let cb = self.on_packet.cb.load(SeqCst);
        let opaque = self.on_packet.opaque.load(SeqCst);
        if !cb.is_null() {
            (cb as XtrClientPacketHandler)(Box::into_raw(Box::new(packet)), opaque);
        }
    }

    fn on_state(&self, state: ClientState) {
        let cb = self.on_state.cb.load(SeqCst);
        let opaque = self.on_state.opaque.load(SeqCst);
        if !cb.is_null() {
            (cb as XtrClientStateHandler)(state, opaque);
        }
    }
}

struct ClientCtx {
    handler: Arc<MyHandler>,
    client: Client,
}

pub type XtrClientRef = Arc<ClientCtx>;

#[no_mangle]
pub unsafe extern "C" fn xtr_client_new(addr: *const c_char, flags: u32) -> *mut XtrClientRef {
    let handler = Arc::new(MyHandler {});
    client = Client::new("192.168.2.252:9900", handler);
    let ctx = Arc::new(ClientCtx { handler, client });
}

#[no_mangle]
pub unsafe extern "C" fn xtr_client_set_packet_cb(
    xtr: *mut XtrClientRef,
    cb: XtrClientPacketHandler,
    opaque: *mut c_void,
) {
}

#[no_mangle]
pub unsafe extern "C" fn xtr_client_set_state_cb(
    xtr: *mut XtrClientRef,
    cb: XtrClientStateHandler,
    opaque: *mut c_void,
) {
}

#[no_mangle]
pub unsafe extern "C" fn xtr_client_start(xtr: *mut XtrClientRef) -> i32 {}

#[no_mangle]
pub unsafe extern "C" fn xtr_client_stop(xtr: *mut XtrClientRef) -> i32 {}

#[no_mangle]
pub unsafe extern "C" fn xtr_client_ref(xtr: *mut XtrClientRef) -> *mut XtrClientRef {}

#[no_mangle]
pub unsafe extern "C" fn xtr_client_unref(xtr: *mut XtrClientRef) {}

#[no_mangle]
pub unsafe extern "C" fn xtr_packet_new_data(
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

pub unsafe extern "C" fn xtr_packet_flags(packet: *const XtrPacketRef) -> u8 {
    (&*packet).flags().bits()
}

pub unsafe extern "C" fn xtr_packet_length(packet: *const XtrPacketRef) -> u32 {
    (&*packet).length()
}

pub unsafe extern "C" fn xtr_packet_type(packet: *const XtrPacketRef) -> u8 {
    (&*packet).type_() as u8
}

#[no_mangle]
pub unsafe extern "C" fn xtr_packet_ref(packet: *mut XtrPacketRef) -> *mut XtrPacketRef {
    let refer = Arc::clone(&*packet);
    Box::into_raw(Box::new(refer))
}

#[no_mangle]
pub unsafe extern "C" fn xtr_packet_unref(packet: *mut XtrPacketRef) {
    let _ = Box::from_raw(packet);
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
