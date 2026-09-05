//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![feature(non_lifetime_binders)]
#![allow(dead_code, incomplete_features)]

trait Project { type Output; }
fn projection<T: Project>(value: T::Output) -> u32 {
    let result;
    result = Some(value);
    result.unwrap()
    //~^ ERROR mismatched types
}

fn error() {
    let value: Missing = 0;
    //~^ ERROR cannot find type `Missing` in this scope
    let result;
    result = Some(value);
    let _ = result;
}

trait Same<T> {}
impl<T> Same<T> for T {}
fn require<U>() where for<T> T: Same<U> {}
fn placeholder() {
    require::<_>();
    //~^ ERROR the trait bound `T: Same<_>` is not satisfied
}

fn main() {}
