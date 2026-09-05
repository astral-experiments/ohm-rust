//@ check-pass
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
    // The terminal Leaf proof reaches, but does not exceed, the limit.
    require::<Layer<Layer<Leaf<'a>>>>();
}
fn main() {}
