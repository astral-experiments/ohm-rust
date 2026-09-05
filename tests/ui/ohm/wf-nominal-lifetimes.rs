//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![allow(dead_code)]

struct Borrowed<'a, T>(&'a T);
struct Explicit<'a, 'b: 'a>(&'a u32, &'b u32);

fn require<T>() {}

fn inferred<'a, T>() {
    require::<Option<Borrowed<'a, T>>>();
    //~^ ERROR the parameter type `T` may not live long enough
}

fn explicit<'a, 'b>() {
    require::<Result<Explicit<'a, 'b>, ()>>();
    //~^ ERROR lifetime may not live long enough
}

fn main() {}
