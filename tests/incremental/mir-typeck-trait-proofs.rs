//@ revisions: bpass1 bfail2
//@ compile-flags: -Zincremental-verify-ich
//@[bfail2] compile-flags: -Zassert-incr-state=loaded

#![allow(dead_code)]

trait Required {}
#[cfg(bpass1)]
impl Required for &u32 {}
#[cfg(bfail2)]
impl Required for &'static u32 {}

fn require<T: Required>(_: T) {}

// Both unchanged bodies must record the trait proof's dependencies, including when
// the second body finds the proof in the worker-local cache populated by the first.
fn first(value: &u32) {
    require(value);
    //[bfail2]~^ ERROR borrowed data escapes outside of function
}

fn second(value: &u32) {
    require(value);
    //[bfail2]~^ ERROR borrowed data escapes outside of function
}

fn main() {}
