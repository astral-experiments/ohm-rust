//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![allow(dead_code)]

struct Invalid(Invalid);
//~^ ERROR recursive type `Invalid` has infinite size

fn require<T>() {}

fn use_invalid() {
    require::<Invalid>();
}

fn main() {}
