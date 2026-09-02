#![feature(rustc_attrs)]
#![allow(internal_features)]

#[rustc_force_inline]
fn increment(value: u32) -> u32 {
    value + 1
}

pub fn optimized(value: u32) -> u32 {
    increment(value) + (value * 0)
}

pub fn optional(value: Option<u32>) -> u32 {
    value.map(increment).unwrap_or(0)
}

pub unsafe fn read_pointer(value: *const u32) -> u32 {
    unsafe { *value }
}
