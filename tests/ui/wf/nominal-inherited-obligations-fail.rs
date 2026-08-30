//@ revisions: current next
//@[next] compile-flags: -Znext-solver
//@ ignore-compare-mode-next-solver (explicit revisions)

#![allow(dead_code)]

// Errors must still point to the bounds on the defining parent and associated item.
trait Parent<T: Copy> {
    type Item<U: Clone>;
}

fn inherited<T, U, P>()
where
    P: Parent<T>,
    //~^ ERROR the trait bound `T: Copy` is not satisfied
{
    let _: Option<<P as Parent<T>>::Item<U>>;
    //~^ ERROR the trait bound `T: Copy` is not satisfied
    //~| ERROR the trait bound `U: Clone` is not satisfied
    //[next]~| ERROR the trait bound `T: Copy` is not satisfied
}

fn main() {}
