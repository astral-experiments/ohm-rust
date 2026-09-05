//@ revisions: bpass1 bfail2
//@ compile-flags: -Zincremental-verify-ich
//@[bfail2] compile-flags: -Zassert-incr-state=loaded

#![allow(dead_code)]

use std::marker::PhantomData;

#[cfg(bpass1)]
struct Decl<'a>(PhantomData<&'a ()>);
#[cfg(bfail2)]
struct Decl<'a: 'static>(PhantomData<&'a ()>);

fn require<T>() {}

fn first<'a>() {
    require::<Option<Decl<'a>>>();
    //[bfail2]~^ ERROR lifetime may not live long enough
}

fn second<'a>() {
    require::<Result<Decl<'a>, u8>>();
    //[bfail2]~^ ERROR lifetime may not live long enough
}

fn main() {}
