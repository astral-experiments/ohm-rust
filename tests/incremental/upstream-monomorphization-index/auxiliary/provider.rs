//@ no-prefer-dynamic
//@ compile-flags: -Zshare-generics=yes

#![crate_type = "rlib"]

#[inline(never)]
pub fn width<T>() -> usize {
    std::hint::black_box(std::mem::size_of::<T>())
}

pub fn widths() -> (usize, usize) {
    #[cfg(not(rpass3))]
    let second = width::<u64>();
    #[cfg(rpass3)]
    let second = width::<u32>();
    (width::<u8>(), second)
}
