//@ check-pass
//@ revisions: current next
//@[next] compile-flags: -Znext-solver

#![allow(dead_code)]

type Pair<T> = (T, T);
type Repeated<T> = Pair<Pair<Pair<Pair<Pair<Pair<T>>>>>>;

#[derive(Clone, Copy)]
struct Bounded<T: Copy>(T);

trait Identity {
    type Output;
    fn identity(self) -> Self::Output;
}

impl<T> Identity for Pair<T> {
    type Output = Self;
    fn identity(self) -> Self::Output { self }
}

// Repeated types retain each distinct lifetime, type variable, and const argument
// when their associated output is normalized and related to the return type.
fn project<'a, 'b, T: Copy, const N: usize>(
    value: Repeated<(&'a [T; N], &'b [T; N], Bounded<T>)>,
) -> Repeated<(&'a [T; N], &'b [T; N], Bounded<T>)> {
    value.identity()
}

fn higher_ranked<F>(_: F)
where
    F: for<'a, 'b> Fn(Repeated<(&'a [u8; 4], &'b [u8; 4])>) -> &'a [u8; 4],
{}

fn first<'a, 'b>(value: Repeated<(&'a [u8; 4], &'b [u8; 4])>) -> &'a [u8; 4] {
    value.0.0.0.0.0.0.0
}

fn inferred(value: &[u8; 4]) {
    let pair = (value, value, Bounded(0u8));
    let pair = (pair, pair);
    let pair = (pair, pair);
    let pair = (pair, pair);
    let pair = (pair, pair);
    let pair = (pair, pair);
    let pair = (pair, pair);
    let _ = project(pair);
}

fn main() {
    higher_ranked(first);
    inferred(&[0; 4]);
}
