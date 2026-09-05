//@ revisions: current next
//@[next] compile-flags: -Znext-solver

#![allow(dead_code)]

type Pair<T> = (T, T);
type Repeated<T> = Pair<Pair<Pair<Pair<Pair<Pair<T>>>>>>;

struct Bounded<T: Copy>(T);

// Repeated nominal types must still require their bounds.
fn missing_bound<T>(_: Repeated<Bounded<T>>) {}
//~^ ERROR the trait bound `T: Copy` is not satisfied

// Repeated reference types must not equate independent lifetimes.
fn distinct_lifetimes<'a, 'b>(value: Repeated<(&'a u8, &'b u8)>) -> &'b u8 {
    value.0.0.0.0.0.0.0
    //~^ ERROR lifetime may not live long enough
}

fn main() {}
