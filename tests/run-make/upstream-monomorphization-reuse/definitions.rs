#![crate_name = "mono_defs"]
#![crate_type = "rlib"]

#[inline(never)]
pub fn identity<T: Copy>(value: T) -> T {
    value
}
