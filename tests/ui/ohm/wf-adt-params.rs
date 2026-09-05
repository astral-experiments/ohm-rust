//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![allow(dead_code)]

struct SizedType<T>(T);
struct ConstType<const N: usize>;

fn require<T>() {}

fn unsized_argument() {
    require::<SizedType<str>>();
    //~^ ERROR the size for values of type `str` cannot be known at compilation time
    //[next]~| ERROR the size for values of type `str` cannot be known at compilation time
}

fn wrong_const_type<const N: u32>() {
    require::<ConstType<N>>();
    //~^ ERROR the constant `N` is not of type `usize`
    //[next]~| ERROR the constant `N` is not of type `usize`
}

fn main() {}
