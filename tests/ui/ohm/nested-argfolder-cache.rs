//@ run-pass
//@ revisions: current next
//@[next] compile-flags: -Znext-solver

// The same composite `(T,)` appears at binder depths 0, 1, 2, then 0 again.
// Substituting a bound lifetime must shift it separately at each depth.
type Depths<T> = ((T,), fn((T,)), fn(fn((T,))), (T,));
type BoundDepths = for<'a> fn(Depths<&'a u8>);
type ExpandedDepths = for<'a> fn(((&'a u8,), fn((&'a u8,)), fn(fn((&'a u8,))), (&'a u8,)));

fn expand(value: BoundDepths) -> ExpandedDepths {
    value
}

fn contract(value: ExpandedDepths) -> BoundDepths {
    value
}

type Arrays<T, const N: usize> = ([T; N], [T; N]);
type References<'a, T> = ((&'a T,), (&'a T,));

fn references<T>(value: &T) -> References<'_, T> {
    ((value,), (value,))
}

fn three<T, U, V>(value: (T, U, V)) -> (T, U, V) {
    value
}

fn main() {
    let bound: BoundDepths = |(first, _, _, last)| {
        assert!(std::ptr::eq(first.0, last.0));
    };
    let value = 7;
    contract(expand(bound))(((&value,), |_| {}, |_| {}, (&value,)));

    // Two arguments include both the type and const value, or the lifetime and type.
    let short: Arrays<u8, 2> = ([1, 2], [3, 4]);
    let long: Arrays<u8, 3> = ([5, 6, 7], [8, 9, 10]);
    let wide: Arrays<u16, 2> = ([256, 257], [258, 259]);
    assert_eq!(short.0, [1, 2]);
    assert_eq!(long.1, [8, 9, 10]);
    assert_eq!(wide.0, [256, 257]);
    let first = references(&short.0);
    let second = references(&wide.1);
    assert_eq!(*first.0.0, [1, 2]);
    assert_eq!(*second.1.0, [258, 259]);

    // A longer argument list must retain the ordinary instantiation behavior.
    assert_eq!(three((11u8, 12u16, 13u32)), (11, 12, 13));
}
