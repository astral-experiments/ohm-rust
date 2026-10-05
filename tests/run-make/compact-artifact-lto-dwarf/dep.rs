#[inline(never)]
pub fn dependency_value(value: u32) -> u32 {
    std::hint::black_box(value + 1)
}
