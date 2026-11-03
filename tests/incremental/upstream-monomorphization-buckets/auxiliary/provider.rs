//@ no-prefer-dynamic
//@ compile-flags: -Zshare-generics=yes

#![crate_type = "rlib"]

#[inline(never)]
pub fn width<T>() -> usize {
    std::hint::black_box(std::mem::size_of::<T>())
}

pub fn widths() -> (usize, usize) {
    #[cfg(rpass1)]
    let other = width::<u16>();
    #[cfg(rpass2)]
    let other = width::<u32>();
    (width::<u8>(), other)
}
