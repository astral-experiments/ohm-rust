unsafe extern "Rust" {
    fn other_unit() -> u64;
}

pub fn caller() -> u64 {
    unsafe { other_unit() }.wrapping_add(1)
}
