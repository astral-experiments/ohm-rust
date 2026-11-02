#[inline(never)]
pub fn measure<T: Into<u32>>(value: T) -> u32 {
    value.into() + std::mem::size_of::<T>() as u32
}

pub fn measured() -> u32 {
    measure(7u16)
}
