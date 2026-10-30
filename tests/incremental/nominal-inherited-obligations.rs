//@ revisions: bpass1 bfail2 bpass3
//@ compile-flags: -Zincremental-verify-ich
//@[bfail2] compile-flags: -Zassert-incr-state=loaded
//@[bpass3] compile-flags: -Zassert-incr-state=loaded

#![allow(dead_code)]

// The consumer stays unchanged while the parent and associated item gain and then lose
// bounds. Reusing its earlier obligations must not hide the new inherited requirements.
#[cfg(any(bpass1, bpass3))]
trait Parent<T> {
    type Item<U>;
}

#[cfg(bfail2)]
trait Parent<T: Copy> {
    type Item<U: Clone>;
}

fn projected<T, U, P: Parent<T>>() {
    //[bfail2]~^ ERROR the trait bound `T: Copy` is not satisfied
    let _: Option<<P as Parent<T>>::Item<U>>;
    //[bfail2]~^ ERROR the trait bound `T: Copy` is not satisfied
    //[bfail2]~| ERROR the trait bound `U: Clone` is not satisfied
}

fn main() {}
