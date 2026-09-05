//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths

#![allow(dead_code)]

trait StaticOnly {}
impl StaticOnly for &'static u32 {}

fn require<T: StaticOnly>(_: T) {}

fn first(value: &'static u32) { require(value); }
fn second(value: &'static u32) { require(value); }

// Reusing proofs from other bodies must preserve each body's region requirements.
fn shorter(value: &u32) {
    require(value);
    //~^ ERROR borrowed data escapes outside of function
}

// The caller bounds are part of the key, including their lifetime identities.
fn bounded<'a: 'static>(value: &'a u32) { require(value); }
fn unbounded<'a>(value: &'a u32) {
    require(value);
    //~^ ERROR borrowed data escapes outside of function
}

fn main() {}
