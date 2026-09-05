//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

fn require<T>() {}
struct NeedsClone<T: Clone>(T);
struct NotClone;

fn nominal_bound<'a>() {
    require::<&'a NeedsClone<NotClone>>();
    //~^ ERROR the trait bound `NotClone: Clone` is not satisfied
    //[next]~| ERROR the trait bound `NotClone: Clone` is not satisfied
}

trait Project { type Output; }
fn projection<'a, T: Project>() {
    require::<&'a T::Output>();
    //~^ ERROR the associated type `<T as Project>::Output` may not live long enough
}

fn unsupported<'a, 'b>() {
    // The outer reference is collected before the unsupported tuple forces fallback.
    require::<&'a (&'b str,)>();
    //~^ ERROR lifetime may not live long enough
}

fn require_forall<T>(_: T) where for<'a> T: 'a {}
fn higher_ranked<'a>(value: &'a str) {
    require_forall(value);
    //~^ ERROR borrowed data escapes outside of function
}

fn main() {}
