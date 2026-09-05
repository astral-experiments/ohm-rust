//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![allow(dead_code)]

struct Local<'a>(&'a u32);

fn require<F: for<'a> FnOnce(Local<'a>)>(_: F) {}

fn only_static(_: Local<'static>) {}

fn main() {
    require(only_static);
    //[current]~^ ERROR implementation of `FnOnce` is not general enough
    //[slow]~^^ ERROR implementation of `FnOnce` is not general enough
    //[next]~^^^ ERROR expected an `FnOnce(Local<'a>)` closure
}
