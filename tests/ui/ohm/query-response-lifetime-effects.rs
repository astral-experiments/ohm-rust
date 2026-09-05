//@ revisions: current next
//@[next] compile-flags: -Znext-solver

#![allow(dead_code, dropping_copy_types)]

#[derive(Clone)]
struct StaticOnly<'a>(&'a u8);
impl Copy for StaticOnly<'static> {}

fn copy_requires_static() {
    let value = 3;
    let wrapped = StaticOnly(&value);
    //~^ ERROR `value` does not live long enough
    // A successful Copy proof can still impose a lifetime constraint.
    drop(wrapped);
    drop(wrapped);
}

fn higher_ranked<'a>(value: &'a u8) -> impl for<'b> Fn(&'b u8) -> &'b u8 {
    // A captured reference cannot be returned for every argument lifetime.
    move |_| value
    //~^ ERROR lifetime may not live long enough
}

fn main() {}
