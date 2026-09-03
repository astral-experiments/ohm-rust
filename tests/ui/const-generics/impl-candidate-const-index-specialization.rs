//@ run-pass

#![feature(min_specialization)]

struct Input<const N: usize>;

trait Read {
    fn read() -> usize;
}

macro_rules! values {
    ($($n:literal),*) => {$(
        impl Read for Input<$n> {
            fn read() -> usize { $n }
        }
    )*};
}
values!(0, 1, 2, 3, 4, 5, 6, 7);
impl<const N: usize> Read for Input<N> {
    default fn read() -> usize { N + 100 }
}
values!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23);

fn generic<const N: usize>() -> usize {
    Input::<N>::read()
}

fn main() {
    assert_eq!(Input::<3>::read(), 3);
    assert_eq!(Input::<17>::read(), 17);
    assert_eq!(generic::<1000>(), 1100);
}
