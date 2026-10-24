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

fn reject_forward<'short, 'long>(
    nested: &'short &'long (),
    value: &'short (),
) -> impl Copy + 'long {
    forward(nested, value, &())
    //~^ ERROR lifetime may not live long enough
}

fn reject_reverse<'short, 'long>(
    nested: &'short &'long (),
    value: &'short (),
) -> impl Copy + 'long {
    reverse(nested, value, &())
    //~^ ERROR lifetime may not live long enough
}

fn main() {}
