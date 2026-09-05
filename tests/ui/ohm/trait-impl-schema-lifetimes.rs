//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

trait Ordinary {}
fn require<T: Ordinary>(_: T) {}

struct Fixed<'a>(&'a u8);
impl Ordinary for Fixed<'static> {}
fn fixed<'a>(value: Fixed<'a>) {
    require(value);
    //~^ ERROR borrowed data escapes outside of function
}

struct Restricted<'a>(&'a u8);
impl<'a: 'static> Ordinary for Restricted<'a> {}
fn restricted<'a>(value: Restricted<'a>) {
    require(value);
    //~^ ERROR borrowed data escapes outside of function
}

struct Repeated<'a, 'b>(&'a u8, &'b u8);
impl<'a> Ordinary for Repeated<'a, 'a> {}
fn repeated<'a, 'b>(value: Repeated<'a, 'b>) {
    require::<Repeated<'a, 'b>>(value);
    //~^ ERROR lifetime may not live long enough
    //~| ERROR lifetime may not live long enough
}

fn main() {}
