//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![allow(dead_code)]

struct MissingClone;
impl Copy for MissingClone {}
//~^ ERROR the trait bound `MissingClone: Clone` is not satisfied

fn duplicate(value: &MissingClone) -> MissingClone {
    *value
}

fn main() {}
