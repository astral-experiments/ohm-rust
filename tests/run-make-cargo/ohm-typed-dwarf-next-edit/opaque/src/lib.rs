#[inline(never)]
#[track_caller]
pub fn location() -> u32 {
    std::hint::black_box(core::panic::Location::caller()).line()
}
