//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![allow(dead_code)]

#[derive(Clone)]
struct Fixed<'a>(&'a u32);
impl Copy for Fixed<'static> {}

fn fixed<'a>(value: &Fixed<'a>) -> Fixed<'a> {
    *value
    //~^ ERROR lifetime may not live long enough
}

#[derive(Clone)]
struct Duplicated<'a, 'b>(&'a u32, &'b u32);
impl<'a> Copy for Duplicated<'a, 'a> {}

fn duplicated<'a, 'b>(value: &Duplicated<'a, 'b>) -> Duplicated<'a, 'b> {
    *value
    //~^ ERROR lifetime may not live long enough
    //~| ERROR lifetime may not live long enough
}

#[derive(Clone)]
struct Restricted<'a>(&'a u32);
impl<'a: 'static> Copy for Restricted<'a> {}

fn restricted<'a>(value: &Restricted<'a>) -> Restricted<'a> {
    *value
    //~^ ERROR lifetime may not live long enough
}

#[derive(Copy, Clone)]
struct Unconditional<'a>(&'a u32);

// Selection prefers the caller bound, which relates the two lifetimes.
fn caller_bound<'a, 'b>(value: &Unconditional<'b>) -> Unconditional<'b>
where
    Unconditional<'a>: Copy,
{
    *value
    //~^ ERROR lifetime may not live long enough
    //~| ERROR lifetime may not live long enough
}

fn main() {}
