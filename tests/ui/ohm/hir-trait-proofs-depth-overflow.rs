//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![recursion_limit = "2"]
#![allow(dead_code)]

trait Ordinary {}
struct Leaf<'a>(&'a ());
impl Ordinary for Leaf<'_> {}
struct Layer<T: ?Sized>(*const T);
impl<T: Ordinary + ?Sized> Ordinary for Layer<T> {}
fn require<T: Ordinary>() {}

fn check<'a>(_: &'a ()) {
    // The terminal Leaf impl is unconditional, but its proof exceeds the limit.
    require::<Layer<Layer<Layer<Leaf<'a>>>>>();
    //~^ ERROR overflow evaluating the requirement
}
fn main() {}
