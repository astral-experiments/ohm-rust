//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

fn require_static(_: (), _: &'static u32, _: bool) {}
fn identity(value: &u32, _: ()) -> &u32 { value }
fn tuple<T>(_: (), _: (Option<T>,)) {}
fn nominal<T>(_: (), _: Option<T>, _: bool) {}
fn output<T>(_: u32) -> Option<T> { None }

fn trivial_input(value: &u32) {
    require_static((), value, false);
    //~^ ERROR borrowed data escapes outside of function
}

fn trivial_output(value: &u32) -> &'static u32 {
    identity(value, ())
    //~^ ERROR lifetime may not live long enough
}

fn mixed_tuple<'a, 'b>() {
    tuple::<&'a &'b ()>((), (None,));
    //~^ ERROR lifetime may not live long enough
}

fn mixed_nominal<'a, 'b>() {
    nominal::<&'a &'b ()>((), None, false);
    //~^ ERROR lifetime may not live long enough
}

fn mixed_output<'a, 'b>() {
    output::<&'a &'b ()>(0);
    //~^ ERROR lifetime may not live long enough
}

fn main() {}
