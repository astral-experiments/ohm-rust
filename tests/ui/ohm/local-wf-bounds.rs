//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

struct SizedOnly<T>(T);
struct CopyOnly<T: Copy>(T);
trait NotDyn { fn generic<T>(&self); }

fn failed_sized<T: ?Sized>() {
    let _ = None::<SizedOnly<T>>;
    //~^ ERROR the size for values of type `T` cannot be known at compilation time
    //[next]~^^ ERROR the size for values of type `T` cannot be known at compilation time
}

fn failed_copy<'a>() {
    // The later nominal bound must reject the plan even after visiting
    // the reference's outlives requirement.
    let _ = None::<(&'a (), CopyOnly<String>)>;
    //~^ ERROR the trait bound `String: Copy` is not satisfied
    //[next]~^^ ERROR the trait bound `String: Copy` is not satisfied
}

fn failed_dyn() {
    let _ = None::<&dyn NotDyn>;
    //~^ ERROR the trait `NotDyn` is not dyn compatible
}

fn main() {}
