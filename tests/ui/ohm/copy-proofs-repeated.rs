//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths

#[derive(Clone, Copy)]
struct Borrowed<'a, T: Copy> {
    value: T,
    borrowed: &'a T,
}

fn repeated<T: Copy>(value: T) -> (T, T, T, T) {
    (value, value, value, value)
}

fn identity(value: &u32) -> &u32 { value }

fn main() {
    let value = 7u32;
    let data = Borrowed { value, borrowed: &value };
    let copies = repeated(data);
    assert_eq!(copies.0.value + copies.1.value, 14);
    assert_eq!(*copies.2.borrowed + *copies.3.borrowed, 14);

    let function: for<'a> fn(&'a u32) -> &'a u32 = identity;
    let functions = repeated(function);
    assert_eq!(functions.0(&value), functions.3(&value));
    let references = repeated(&value);
    assert_eq!(*references.0, *references.3);
}
