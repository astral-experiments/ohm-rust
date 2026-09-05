//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

fn escape<'a>(value: &'a u32) -> Option<&'static u32> {
    let result;
    result = Some((0_u32, value));
    Some(result.unwrap().1)
    //~^ ERROR lifetime may not live long enough
}

fn invariant<'a>(values: &mut Vec<&'static str>, value: &'a str) {
    let result;
    result = Some(values);
    //~^ ERROR lifetime may not live long enough
    result.unwrap().push(value);
}

fn mutability(value: &u32) -> Option<&mut u32> {
    let result;
    result = Some(value);
    result
    //~^ ERROR mismatched types
}

fn require_higher_ranked(_: for<'a> fn(&'a u32) -> &'a u32) {}
fn higher_ranked(value: fn(&'static u32) -> &'static u32) {
    let result;
    result = Some(value);
    require_higher_ranked(result.unwrap());
    //~^ ERROR mismatched types
}

fn main() {}
