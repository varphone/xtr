use xtr::PackedValues;

fn main() {
    let mut pv = PackedValues::new();
    pv.put_u8s(0x0101, &b"1234567"[..]);
    pv.put_u8(0x0100, 0xAA);
    pv.put_u32(0x0001, 0xAABBCCDD);
    pv.put_u32s(0x0000, &[0xAABBCCDD, 0x11223344]);
    pv.put_f32s(0x0002, &[0.1, 0.2]);
    println!("{:x?}", pv.as_bytes());
    println!("{:x?}", pv.get_u8(0x0100));
    println!("{:x?}", pv.get_u32(0x0000));
    println!("{:x?}", pv.get_u32(0x0001));
    println!("{:x?}", pv.get_u32s(0x0000, 2));
    println!("{:?}", pv.get_u32s(0x0002, 2));
    for a in pv.items() {
        println!("a={:?}", a);
    }
}
