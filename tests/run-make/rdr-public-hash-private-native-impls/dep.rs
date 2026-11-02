#![crate_type = "rlib"]

mod hidden;

pub fn layout<T>(_: T) -> usize {
    std::mem::size_of::<hidden::Layout>()
}

#[inline(never)]
pub fn value() -> u8 {
    hidden::converted()
}
