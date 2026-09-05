//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![recursion_limit = "256"]

struct Owned(String);
struct Borrowed<'a>(&'a str);
fn require<T>() {}
fn identity<T>(value: T) -> T { value }

fn generic<'a, T: 'a>(value: &'a T) -> &'a T {
    require::<&'a T>();
    require::<Option<&'a T>>();
    require::<Result<&'a T, &'a T>>();
    require::<*const &'a T>();
    identity(value)
}

fn nested<'a, 'b: 'a, T: 'b>(value: &'a &'b T) -> &'a &'b T {
    require::<Option<&'a &'b T>>();
    identity(Some(value)).unwrap()
}

fn nominal<'a>(value: &'a Borrowed<'a>) -> &'a Borrowed<'a> {
    require::<&'a Borrowed<'a>>();
    require::<*const Borrowed<'a>>();
    require::<*mut Borrowed<'a>>();
    identity(value)
}

macro_rules! references {
    ($region:lifetime, $ty:ty;) => { $ty };
    ($region:lifetime, $ty:ty; $head:tt $($tail:tt)*) => {
        &$region references!($region, $ty; $($tail)*)
    };
}

fn budget<'a, T: 'a>() {
    // More than 64 reference nodes fall back after collecting only a partial plan.
    require::<references!('a, T;
        x x x x x x x x x x x x x x x x
        x x x x x x x x x x x x x x x x
        x x x x x x x x x x x x x x x x
        x x x x x x x x x x x x x x x x
        x x x x x x x x
    )>();
}

fn main() {
    let owned = Owned(String::from("owned"));
    assert_eq!(generic(&owned).0, "owned");
    let borrowed = Borrowed(&owned.0);
    assert_eq!(nominal(&borrowed).0, "owned");
    let reference = &borrowed;
    assert_eq!(nested(&reference).0, "owned");
    budget::<String>();
}
