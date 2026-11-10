#[inline(never)]
pub fn source<T>() -> &'static str {
    std::hint::black_box(std::mem::size_of::<T>());
    std::panic::Location::caller().file()
}
