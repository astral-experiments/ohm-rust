//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

trait Ordinary {}
struct Token<'a>(&'a u8);
impl Ordinary for Token<'_> {}
fn require<T: Ordinary>(_: T) {}

// The applicable caller bound relates the lifetimes despite the unconditional impl.
fn caller_bound<'a, 'b>(value: Token<'b>)
where
    Token<'a>: Ordinary,
{
    require::<Token<'b>>(value);
    //~^ ERROR lifetime may not live long enough
    //~| ERROR lifetime may not live long enough
}

trait Outlives: 'static {}
// The impl uses the ADT's implied bound; its own where-clause is empty.
struct Bounded<'a: 'static>(&'a u8);
impl<'a> Outlives for Bounded<'a> {}
fn require_outlives<T: Outlives>() {}
fn super_outlives<'a>(_: &'a u8) {
    require_outlives::<Bounded<'a>>();
    //~^ ERROR lifetime may not live long enough
}

fn main() {}
