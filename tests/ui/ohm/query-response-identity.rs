//@ run-pass
//@ revisions: current next
//@[next] compile-flags: -Znext-solver

fn duplicate<'a, T>(value: &'a T) -> (&'a T, &'a T) {
    (value, value)
}

fn apply_twice<'a, F>(function: F, value: &'a u8) -> (&'a u8, &'a u8)
where
    F: Copy + for<'b> Fn(&'b u8) -> &'b u8,
{
    let other = function;
    (function(value), other(value))
}

fn identity(value: &u8) -> &u8 { value }

fn main() {
    // Copy proofs for references may reuse their original regions. Each call's
    // result still has to relate those regions to its own arguments and return type.
    let first = 3;
    let second = 7;
    let (a, b) = duplicate(&first);
    let (c, d) = duplicate(&second);
    assert_eq!((*a, *b, *c, *d), (3, 3, 7, 7));

    // Higher-ranked callable bounds retain their binders when the surrounding
    // query response has no region effects.
    let (a, b) = apply_twice(identity, &first);
    assert_eq!((*a, *b), (3, 3));
    let function: for<'a> fn(&'a u8) -> &'a u8 = identity;
    let (a, b) = apply_twice(function, &second);
    assert_eq!((*a, *b), (7, 7));
}
