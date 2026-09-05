//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![recursion_limit = "256"]

struct Leaf<'a>(&'a str);
struct Empty;
struct Wrap<T>(T);
struct Meta<T: ?Sized>(T);

fn identity<T>(value: T) -> T {
    value
}

fn require_wf<T: ?Sized>() {}

// A generic caller may need its own Sized bound, and can still use normal selection.
fn generic<T>(value: Option<T>) -> Option<T> {
    identity(value)
}

macro_rules! nested {
    ($value:expr;) => { $value };
    ($value:expr; $head:ident $($tail:ident)*) => { Wrap(nested!($value; $($tail)*)) };
}

fn main() {
    let text = String::from("local");
    let value = Some(Ok(Leaf(&text)..Leaf(&text)));
    let value = identity::<Option<Result<std::ops::Range<Leaf<'_>>, Empty>>>(value);
    let Some(Ok(range)) = value else { unreachable!() };
    assert_eq!(range.start.0, range.end.0);
    assert_eq!(generic(Some(42)), Some(42));

    // ?Sized is a MetaSized requirement, so str and slices are valid arguments here.
    require_wf::<Meta<str>>();
    require_wf::<Meta<[u8]>>();

    // Deeply nested types still work after the bounded proof gives up.
    let deep = nested!(Empty;
        x x x x x x x x x x x x x x x x
        x x x x x x x x x x x x x x x x
        x x x x x x x x x x x x x x x x
        x x x x x x x x x x x x x x x x
        x x x x x x x x
    );
    let _ = identity(deep);
}
