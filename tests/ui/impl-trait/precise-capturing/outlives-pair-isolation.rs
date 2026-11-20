//@ check-pass
//@ edition: 2024
//@ revisions: current next
//@[next] compile-flags: -Znext-solver

#![allow(dead_code)]

fn forward<'short: 'short, 'long: 'long, 'other: 'other>(
    _: &'short &'long (),
    value: &'short (),
    _: &'other (),
) -> impl Copy + 'short + use<'short, 'long, 'other> {
    value
}

fn reverse<'other: 'other, 'long: 'long, 'short: 'short>(
    _: &'short &'long (),
    value: &'short (),
    _: &'other (),
) -> impl Copy + 'short + use<'other, 'long, 'short> {
    value
}

fn needs_outlives<'a, T: 'a>(_: T) {}

fn test<'short, 'long>(nested: &'short &'long (), value: &'short ()) {
    let local = ();
    needs_outlives::<'short, _>(forward(nested, value, &local));
    needs_outlives::<'short, _>(reverse(nested, value, &local));
}

fn main() {}
