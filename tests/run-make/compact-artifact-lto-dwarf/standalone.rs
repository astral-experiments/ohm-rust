#[inline(never)]
fn retained_debug_value(value: u32) -> u32 {
    std::hint::black_box(value + 1)
}

fn main() {
    assert_eq!(retained_debug_value(41), 42);
}
