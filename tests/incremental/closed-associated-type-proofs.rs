//@ revisions: bpass1 bfail2
//@ compile-flags: -Zincremental-verify-ich
//@[bfail2] compile-flags: -Zassert-incr-state=loaded

#![allow(dead_code)]

trait Required {}
#[cfg(bpass1)]
impl Required for [u8; 4] {}

trait Storage { type Value: Required; }
struct A;
struct B;

// Removing a provider must invalidate both unchanged users of the cached proof.
impl Storage for A {
    type Value = [u8; 4];
    //[bfail2]~^ ERROR the trait bound `[u8; 4]: Required` is not satisfied
}
impl Storage for B {
    type Value = [u8; 4];
    //[bfail2]~^ ERROR the trait bound `[u8; 4]: Required` is not satisfied
}

// A shared concrete definition must not hide an associated type's own
// Self-dependent where clause when one impl loses its required bound.
trait Bound {}
struct Good;
struct Bad;
impl Bound for Good {}
#[cfg(bpass1)]
impl Bound for Bad {}

trait ConditionalStorage {
    type Value: Copy
    where
        Self: Bound;
}
impl ConditionalStorage for Good {
    type Value = u8;
}
impl ConditionalStorage for Bad {
    type Value = u8;
    //[bfail2]~^ ERROR the trait bound `Bad: Bound` is not satisfied
}

fn main() {}
