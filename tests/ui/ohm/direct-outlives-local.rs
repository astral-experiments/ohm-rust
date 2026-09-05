//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

fn require<'a, T: 'a>(_: T) {}

fn local_borrow<'a>(_: &'a ()) {
    let local = String::from("local");
    require::<'a, _>(&local);
    //~^ ERROR `local` does not live long enough
}

fn main() {}
