#![crate_type = "rlib"]

#[inline(never)]
pub fn shared<T: Copy>(value: T) -> T {
    value
}
