//@ revisions: bpass1 bfail2
//@ compile-flags: -Zincremental-verify-ich
//@[bfail2] compile-flags: -Zassert-incr-state=loaded

#![allow(dead_code)]

#[derive(Clone)]
struct Value<'a>(&'a u32);

#[cfg(bpass1)]
impl<'a> Copy for Value<'a> {}
#[cfg(bfail2)]
impl Copy for Value<'static> {}

fn first<'a>(value: &Value<'a>) -> Value<'a> {
    *value
    //[bfail2]~^ ERROR lifetime may not live long enough
}

fn second<'a>(value: &Value<'a>) -> Value<'a> {
    *value
    //[bfail2]~^ ERROR lifetime may not live long enough
}

fn main() {}
