#![crate_type = "rlib"]

use std::sync::atomic::{AtomicU8, Ordering};

mod hidden;

static VALUE: AtomicU8 = AtomicU8::new(0);

#[inline(never)]
pub fn observe(x: u64) -> (u8, u64, u64) {
    let (scalar, record) = hidden::call(x);
    (VALUE.load(Ordering::SeqCst), scalar, record)
}
