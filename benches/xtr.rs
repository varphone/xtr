#![feature(test)]
extern crate test;
use test::{black_box, Bencher};
use xtr::{PackedValueKind, PackedValues};

fn get_i32_x(b: &mut Bencher, num: u16) {
    let mut pv = PackedValues::new();
    for addr in 0..num {
        pv.put_i32(addr, addr as i32);
    }
    b.iter(|| {
        for addr in 0..num {
            let r = pv.get_i32(addr);
            assert!(r.is_some())
        }
    });
}

fn get_i64_x(b: &mut Bencher, num: u16) {
    let mut pv = PackedValues::new();
    for addr in 0..num {
        pv.put_i64(addr, addr as i64);
    }
    b.iter(|| {
        for addr in 0..num {
            let r = pv.get_i64(addr);
            assert!(r.is_some())
        }
    });
}

fn peek_i32_x(b: &mut Bencher, num: u16) {
    let mut pv = PackedValues::new();
    for addr in 0..num {
        pv.put_i32(addr, addr as i32);
    }
    b.iter(|| {
        for item in pv.items() {
            match PackedValueKind::from(item.kind) {
                PackedValueKind::I32 => {
                    let r = pv.peek_i32(item.addr, item.ipos as usize);
                    assert!(r.is_some())
                }
                _ => {}
            }
        }
    });
}

fn peek_i64_x(b: &mut Bencher, num: u16) {
    let mut pv = PackedValues::new();
    for addr in 0..num {
        pv.put_i64(addr, addr as i64);
    }
    b.iter(|| {
        for item in pv.items() {
            match PackedValueKind::from(item.kind) {
                PackedValueKind::I64 => {
                    let r = pv.peek_i64(item.addr, item.ipos as usize);
                    assert!(r.is_some())
                }
                _ => {}
            }
        }
    });
}

#[bench]
fn get_i32_1k(b: &mut Bencher) {
    get_i32_x(b, 1024);
}

#[bench]
fn get_i32_4k(b: &mut Bencher) {
    get_i32_x(b, 4096);
}

#[bench]
fn get_i32_16k(b: &mut Bencher) {
    get_i32_x(b, 16 * 1024);
}

#[bench]
fn get_i64_1k(b: &mut Bencher) {
    get_i64_x(b, 1024);
}

#[bench]
fn get_i64_4k(b: &mut Bencher) {
    get_i64_x(b, 4096);
}

#[bench]
fn get_i64_16k(b: &mut Bencher) {
    get_i64_x(b, 16 * 1024);
}

#[bench]
fn peek_i32_1k(b: &mut Bencher) {
    peek_i32_x(b, 1024);
}

#[bench]
fn peek_i32_4k(b: &mut Bencher) {
    peek_i32_x(b, 4096);
}

#[bench]
fn peek_i32_16k(b: &mut Bencher) {
    peek_i32_x(b, 16 * 1024);
}

#[bench]
fn peek_i64_1k(b: &mut Bencher) {
    peek_i64_x(b, 1024);
}

#[bench]
fn peek_i64_4k(b: &mut Bencher) {
    peek_i64_x(b, 4096);
}

#[bench]
fn peek_i64_16k(b: &mut Bencher) {
    peek_i64_x(b, 16 * 1024);
}
