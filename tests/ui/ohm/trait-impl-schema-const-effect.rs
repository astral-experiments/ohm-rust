//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![feature(const_trait_impl)]
#![allow(dead_code)]

const trait Value {
    fn value() -> u8;
}
struct Ordinary;
impl Value for Ordinary {
    fn value() -> u8 { 1 }
}
struct Constant;
const impl Value for Constant {
    fn value() -> u8 { 2 }
}

const fn read<T: [const] Value>() -> u8 {
    T::value()
}
const GOOD: u8 = read::<Constant>();
const BAD: u8 = read::<Ordinary>();
//~^ ERROR the trait bound `Ordinary: const Value` is not satisfied

fn main() {}
