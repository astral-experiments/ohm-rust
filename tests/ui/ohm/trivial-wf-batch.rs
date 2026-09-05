//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

struct Holder<T>(T);

fn unit() {}
fn void(_: (), _: bool, _: u32) {}
fn primitives(value: u32, add: bool, character: char) -> u32 {
    if add { value + character as u32 } else { value }
}

fn references<'a, 'b>(_: &'a str, value: &'b u32) -> &'b u32 {
    value
}

fn slice(value: &[u32], _: (), _: *const ()) -> &[u32] {
    value
}

fn generic<T>(_: u32, value: T, _: ()) -> T {
    value
}

fn identity(value: &u32) -> &u32 {
    value
}

fn function_pointer(value: u32, f: for<'a> fn(&'a u32) -> &'a u32) -> u32 {
    *f(&value)
}

fn mixed<T>(_: (), value: (T,), last: Holder<T>) -> (T, Holder<T>) {
    (value.0, last)
}

fn main() {
    void(unit(), true, primitives(1, true, 'A'));
    assert_eq!(primitives(1, false, 'A'), 1);
    let value = 42;
    let reference = {
        let text = String::from("shorter lifetime");
        references(&text, &value)
    };
    assert_eq!(*reference, value);
    assert_eq!(*references("static", reference), value);
    assert_eq!(slice(&[1, 2], (), std::ptr::null()), &[1, 2]);
    assert_eq!(generic(0, String::from("generic"), ()), "generic");
    assert_eq!(function_pointer(value, identity), value);
    let (first, second) = mixed((), (&value,), Holder(reference));
    assert_eq!(*first, value);
    assert_eq!(*second.0, value);
}
