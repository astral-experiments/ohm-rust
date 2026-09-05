//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

use std::marker::PhantomData;

fn identity<T>(value: T) -> T { value }

fn option<T>(value: Option<T>) -> Option<T> { identity(value) }
fn result<T, E>(value: Result<T, E>) -> Result<T, E> { identity(value) }

struct Required<'a, 'b: 'a, T: 'a>(PhantomData<(&'a (), &'b ())>, T);

fn required<'a, 'b: 'a, T: 'a>(value: T) -> Required<'a, 'b, T> {
    identity(Required(PhantomData, value))
}

trait First<'a>: 'a { fn value(&self) -> u8; }
trait Both<'a, 'b>: First<'a> + 'b {}
impl First<'_> for u8 { fn value(&self) -> u8 { *self } }
impl Both<'_, '_> for u8 {}

fn dyn_bounds<'a, 'b, 'c: 'a + 'b>(value: &'c (dyn Both<'a, 'b> + 'c)) -> u8 {
    identity(value).value()
}

trait Project { type Output; }
impl<'a> Project for &'a () { type Output = &'a (); }

// The input type has no alias, but its nominal predicates do.
struct Projected<'a>(PhantomData<&'a ()>)
where <&'a () as Project>::Output: 'static;

struct ForAll<T>(T) where for<'a> T: 'a;

fn forall<T: 'static>(value: T) -> T { identity(ForAll(value)).0 }
fn array<const N: usize>(value: [u8; N]) -> [u8; N] { identity(value) }

fn consume(_: &str) {}

fn main() {
    assert_eq!(option(Some(String::from("some"))), Some(String::from("some")));
    assert_eq!(result::<_, String>(Ok(7)), Ok(7));
    assert_eq!(result::<u8, _>(Err(String::from("error"))), Err(String::from("error")));

    let text = String::from("borrowed");
    assert_eq!(required(&text).1, "borrowed");
    assert_eq!(dyn_bounds(&9), 9);

    identity(Projected::<'static>(PhantomData));
    assert_eq!(forall(String::from("forall")), "forall");
    assert_eq!(array([1, 2, 3]), [1, 2, 3]);

    // A reference obligation precedes the unsupported array obligation.
    let partial = identity((&text, [4u8, 5]));
    assert_eq!(partial.0, "borrowed");
    assert_eq!(partial.1, [4, 5]);

    // Repeated shared subtrees exceed the structural WF budget.
    let pair = (&text, &text);
    let pair = (pair, pair);
    let pair = (pair, pair);
    let pair = (pair, pair);
    let pair = (pair, pair);
    let pair = (pair, pair);
    assert_eq!(identity(pair).0.0.0.0.0.0, "borrowed");

    let function: for<'a> fn(&'a str) = consume;
    identity(function)("higher-ranked");
}
