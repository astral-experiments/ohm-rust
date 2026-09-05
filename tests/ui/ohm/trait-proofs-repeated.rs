//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths

trait Measure {
    fn measure(self) -> u32;
}

impl Measure for u32 {
    fn measure(self) -> u32 { self }
}

fn generic<T: Measure + Clone + Eq>(value: T) -> u32 {
    assert!(value == value.clone());
    value.measure()
}

fn main() {
    // Repeated calls prove the same concrete user trait, Clone, and Eq bounds.
    assert_eq!(generic(2u32) + generic(3u32) + generic(5u32), 10);
    assert_eq!(generic(7u32) + generic(11u32), 18);
}
