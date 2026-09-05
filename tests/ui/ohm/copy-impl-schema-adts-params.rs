//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![allow(dead_code)]

#[derive(Clone)]
struct NotCopy;

#[derive(Clone, Copy)]
struct Conditional<T>(T);

fn conditional(value: &Conditional<NotCopy>) -> Conditional<NotCopy> {
    *value
    //~^ ERROR cannot move out of `*value` which is behind a shared reference
}

#[derive(Clone)]
struct FixedType<T>(T);
impl Copy for FixedType<u32> {}

fn fixed_type(value: &FixedType<u64>) -> FixedType<u64> {
    *value
    //~^ ERROR cannot move out of `*value` which is behind a shared reference
}

#[derive(Clone)]
struct FixedConst<const N: usize>([u8; N]);
impl Copy for FixedConst<2> {}

fn fixed_const(value: &FixedConst<3>) -> FixedConst<3> {
    *value
    //~^ ERROR cannot move out of `*value` which is behind a shared reference
}

fn main() {}
