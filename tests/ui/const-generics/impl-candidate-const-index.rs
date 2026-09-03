//@ run-pass

use std::marker::PhantomData;

struct Input<'a, T, const N: usize>(PhantomData<&'a T>);

trait Read {
    fn read() -> usize;
}

macro_rules! values {
    ($($n:literal),*) => {$(
        impl Read for Input<'_, u8, $n> {
            fn read() -> usize { $n }
        }
    )*};
}

// Exceed the indexing threshold and interleave fallback impls with value impls.
values!(0, 1, 2, 3, 4, 5, 6, 7);
impl<const N: usize> Read for Input<'_, bool, N> {
    fn read() -> usize { N + 100 }
}
values!(8, 9, 10, 11, 12, 13, 14, 15);
impl<const N: usize> Read for Input<'_, char, N> {
    fn read() -> usize { N + 200 }
}
values!(16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31);

fn generic<const N: usize>() -> usize {
    // A rigid parameter must still find the generic impl, including values absent from the index.
    Input::<'_, bool, N>::read() + Input::<'_, char, N>::read()
}

fn opaque<const N: usize>() -> impl Read {
    Input::<'static, bool, N>(PhantomData)
}

fn read_opaque<T: Read>(_: T) -> usize {
    T::read()
}

fn main() {
    assert_eq!(Input::<'_, u8, 0>::read(), 0);
    assert_eq!(Input::<'_, u8, 17>::read(), 17);
    assert_eq!(Input::<'_, u8, 31>::read(), 31);
    assert_eq!(Input::<'_, bool, 17>::read(), 117);
    assert_eq!(Input::<'_, char, 17>::read(), 217);
    assert_eq!(generic::<1000>(), 2300);
    assert_eq!(read_opaque(opaque::<1000>()), 1100);
}
