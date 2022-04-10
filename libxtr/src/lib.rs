use std::sync::Arc;
use xtr::{Packet, PacketFlags, PacketHead, PacketType};

pub type XtrPacketFlags = PacketFlags;
pub type XtrPacketRef = Arc<Packet>;
pub type XtrPacketType = PacketType;

#[no_mangle]
pub unsafe extern "C" fn xtr_packet_new_data(length: u32, flags: u8, stream_id: u32) -> *mut XtrPacketRef {
    let head = PacketHead::new(length, PacketType::Data, PacketFlags::from(flags), stream_id);
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
