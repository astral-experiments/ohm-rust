//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![allow(unused_assignments)]

fn deferred<T: Default + Copy>() -> (T, T) {
    let first = Default::default();
    // Queue a subtype relation before either variable has a known type.
    let second = first;

    // Constrain both variables in one invariant relation, so the queued
    // relation later resolves both operands to the same type.
    let mut pair = &mut (first, second);
    let replacement = &mut (T::default(), T::default());
    pair = replacement;
    *pair
}

fn main() {
    assert_eq!(deferred::<u32>(), (0, 0));
    assert_eq!(deferred::<[u32; 4]>(), ([0; 4], [0; 4]));
    assert_eq!(deferred::<Option<(u32, [u8; 3])>>(), (None, None));
    assert_eq!(deferred::<&[u8]>(), (&[][..], &[][..]));

    // The function signatures retain distinct inference variables that have
    // already resolved when their parameter and return types are combined.
    let first: fn(_) -> _ = |value| value;
    let second: fn(_) -> _ = |value| value;
    assert_eq!(first([1u32; 4]), [1; 4]);
    assert_eq!(second([2u32; 4]), [2; 4]);
    let common = match 1 {
        0 => first,
        _ => second,
    };
    assert_eq!(common([3u32; 4]), [3; 4]);
}
