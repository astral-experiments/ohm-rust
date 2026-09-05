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

impl Measure for &u32 {
    fn measure(self) -> u32 { *self }
}

fn measure<T: Measure>(value: T) -> u32 { value.measure() }

fn first(value: u32) -> u32 { measure(value) }
fn second(value: u32) -> u32 { measure(value) }

fn first_borrowed(value: &u32) -> u32 { measure(value) }
fn second_borrowed<'a>(value: &'a u32) -> u32 { measure(value) }

// Generic parameters with the same index and name can have different environments.
fn generic<T: Measure>(value: T) -> u32 { measure(value) }
fn generic_clone<T: Measure + Clone>(value: T) -> u32 { measure(value.clone()) }

// Each opaque return type gives the same concrete trait proof a different typing mode.
fn first_opaque(value: u32) -> impl Measure {
    measure(value);
    value
}

fn second_opaque(value: u32) -> impl Measure {
    measure(value);
    value
}

fn main() {
    assert_eq!(first(2) + second(3), 5);
    let value = 7;
    assert_eq!(first_borrowed(&value) + second_borrowed(&value), 14);
    assert_eq!(generic(11u32) + generic_clone(13u32), 24);
    assert_eq!(measure(first_opaque(17)) + measure(second_opaque(19)), 36);
}
