//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![allow(dead_code)]

struct NotClone;
struct NeedsClone<T: Clone>(T);
struct ConstType<const N: usize>;
trait Associated {
    type Output;
}

fn require<T>() {}

fn trait_bound() {
    require::<Option<NeedsClone<NotClone>>>();
    //~^ ERROR the trait bound `NotClone: Clone` is not satisfied
    //[next]~| ERROR the trait bound `NotClone: Clone` is not satisfied
}

fn const_type<const N: u32>() {
    require::<Option<ConstType<N>>>();
    //~^ ERROR the constant `N` is not of type `usize`
    //[next]~| ERROR the constant `N` is not of type `usize`
}

fn alias<T: Associated>() {
    require::<Option<NeedsClone<T::Output>>>();
    //~^ ERROR the trait bound `<T as Associated>::Output: Clone` is not satisfied
    //[next]~| ERROR the trait bound `<T as Associated>::Output: Clone` is not satisfied
}

fn main() {}
