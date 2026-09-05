//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![feature(auto_traits, min_specialization, trait_alias)]

trait Value {
    fn value(&self) -> u8;
}
impl<T> Value for T {
    default fn value(&self) -> u8 { 1 }
}
#[derive(Clone)]
struct Token<'a>(&'a u8);
impl Value for Token<'_> {
    fn value(&self) -> u8 { *self.0 }
}

trait Alias = Value;
auto trait Automatic {}
fn alias<T: Alias + Automatic>(value: &T) -> u8 {
    value.value()
}
fn clone<T: Clone>(value: &T) -> T {
    value.clone()
}

fn main() {
    let two = 2;
    let token = clone(&Token(&two));
    assert_eq!(alias(&token), 2);
    assert_eq!(alias(&0_u8), 1);
}
