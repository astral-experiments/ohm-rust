#![feature(rustc_attrs)]
#![allow(internal_features)]

#[inline(never)]
pub fn ordinary<T: Copy>(value: T) -> T {
    value
}

#[cfg_attr(offload, rustc_offload_kernel)]
#[inline(never)]
pub fn offload_candidate<T: Copy>(value: T) -> T {
    value
}

#[unsafe(no_mangle)]
pub extern "C" fn force_instantiations() -> u32 {
    ordinary(10_u32) + offload_candidate(20_u32)
}
