//@ run-pass
//@ revisions: current next params params_next
//@[next] compile-flags: -Znext-solver
//@[params_next] compile-flags: -Znext-solver
//@ compile-flags: -Zvalidate-mir
//@ ignore-compare-mode-next-solver (explicit revisions)

#![cfg_attr(any(params, params_next), feature(unsized_fn_params))]
#![allow(internal_features)]

struct Tail<T: ?Sized>(T);

trait HasValue {
    type Value;
}

impl HasValue for u8 {
    type Value = String;
}

// Sizedness of references and pointers does not depend on the pointee's regions
// or sizedness. Field projection must still retain the borrowed lifetime.
fn tail<'a, T: ?Sized>(value: &'a Tail<T>) -> &'a T {
    let pointer = value as *const Tail<T>;
    let copy = unsafe { &*pointer };
    &copy.0
}

// An associated type needs the query fallback until its sizedness is known.
fn associated<T: HasValue>(value: T::Value) -> T::Value {
    let temporary = value;
    temporary
}

fn iterator<'a>(values: &'a [&'a str]) -> impl Iterator<Item = &'a str> {
    values.iter().copied().filter(|value| !value.is_empty())
}

// Exercise moves and borrows through dereference, field, downcast, index,
// constant-index and subslice projections.
fn projections(mut values: Box<[Option<String>; 3]>) -> String {
    let [first, rest @ ..] = &mut *values;
    let mut result = first.take().unwrap();
    let index = 1;
    if let Some(value) = &rest[index] {
        result.push_str(value);
    }
    result
}

fn main() {
    let owned = String::from("tail");
    let value = Tail(owned.as_str());
    assert_eq!(*tail(&value), "tail");
    let array_tail = Tail(*b"sized");
    let unsized_tail: &Tail<[u8]> = &array_tail;
    assert_eq!(tail(unsized_tail), b"sized");
    assert_eq!(associated::<u8>(String::from("value")), "value");
    let values = ["", "a", "b"];
    assert_eq!(iterator(&values).collect::<String>(), "ab");
    assert_eq!(projections(Box::new([Some("a".into()), None, Some("b".into())])), "ab");
}
