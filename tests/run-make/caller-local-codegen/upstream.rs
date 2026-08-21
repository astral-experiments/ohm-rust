#![crate_type = "rlib"]

#[inline(never)]
pub fn apply<F: Fn(u32) -> bool>(value: u32, predicate: F) -> bool {
    invoke(value, predicate)
}

#[inline(never)]
fn invoke<F: Fn(u32) -> bool>(value: u32, predicate: F) -> bool {
    predicate(value)
}

#[inline(never)]
pub fn identity<T>(value: T) -> T {
    value
}

pub fn shared_identity() -> u32 {
    identity(11_u32)
}

pub struct Wrapper<F: Fn()>(pub F);

impl<F: Fn()> Drop for Wrapper<F> {
    #[inline(never)]
    fn drop(&mut self) {
        (self.0)();
    }
}
