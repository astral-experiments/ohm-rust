#[inline(never)]
fn cached(value: usize) -> (&'static str, usize) {
    ("retained allocation", value * 2)
}

fn main() {
    let (label, value) = cached(21);
    assert_eq!(label, "retained allocation");
    #[cfg(any(rpass1, rpass2, rpass3))]
    assert_eq!(value, 42);
    #[cfg(any(rpass4, rpass5, rpass6))]
    assert_eq!(value + 1, 43);
    #[cfg(rpass7)]
    assert_eq!(value + 2, 44);
}
