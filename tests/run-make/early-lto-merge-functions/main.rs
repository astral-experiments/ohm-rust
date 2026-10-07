fn main() {
    let value = std::hint::black_box(41_u64);
    assert_eq!(value.wrapping_add(1), 42);
}
