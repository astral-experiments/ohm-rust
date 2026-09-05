//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![allow(dead_code)]

// The field induces the predicate 'b: 'a.
struct Inferred<'a, 'b>(&'a &'b u32);

struct Explicit<'a, 'b: 'a>(&'a u32, &'b u32);

struct Borrowed<'a, T>(&'a T);

fn require<T>() {}

fn inferred<'a, 'b>() {
    require::<Inferred<'a, 'b>>();
    //~^ ERROR lifetime may not live long enough
}

fn explicit<'a, 'b>() {
    require::<Explicit<'a, 'b>>();
    //~^ ERROR lifetime may not live long enough
}

fn type_parameter<'a, T>() {
    require::<Borrowed<'a, T>>();
    //~^ ERROR the parameter type `T` may not live long enough
}

fn main() {}
