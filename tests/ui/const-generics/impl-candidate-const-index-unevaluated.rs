//@ run-pass

#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

use std::marker::PhantomData;

struct Input<T, const N: usize>(PhantomData<T>);

trait Read {
    fn read() -> usize;
}

trait Width {
    const WIDTH: usize;
}

struct Symbolic;
impl Width for Symbolic {
    const WIDTH: usize = 40;
}

impl<T: Width> Read for Input<T, { T::WIDTH }> {
    fn read() -> usize { T::WIDTH }
}

macro_rules! values {
    ($($n:literal),*) => {$(
        impl Read for Input<u8, $n> {
            fn read() -> usize { $n }
        }
    )*};
}
values!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);

impl<const N: usize> Read for Input<bool, N> {
    fn read() -> usize { N }
}

fn symbolic<T: Width>() -> usize
where
    [(); T::WIDTH]:,
{
    // Both the obligation and the impl contain an unevaluated associated const.
    Input::<T, { T::WIDTH }>::read()
}

fn expression<const N: usize>() -> usize
where
    [(); N + 1]:,
{
    Input::<bool, { N + 1 }>::read()
}

fn main() {
    assert_eq!(symbolic::<Symbolic>(), 40);
    assert_eq!(expression::<40>(), 41);
    assert_eq!(Input::<u8, 16>::read(), 16);
}
