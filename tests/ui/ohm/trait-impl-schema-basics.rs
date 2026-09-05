//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![feature(const_trait_impl)]

use std::collections::hash_map::DefaultHasher;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

#[derive(Debug, PartialEq, Eq, Hash)]
enum Name<'a> {
    Empty,
    Borrowed(&'a str),
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct Unit;

trait Ordinary {}
impl Ordinary for Name<'_> {}
impl Ordinary for Unit {}

fn check<T: Ordinary + Hash + Eq + Debug>(left: &T, right: &T) {
    assert_eq!(left, right);
    let mut lhs = DefaultHasher::new();
    let mut rhs = DefaultHasher::new();
    left.hash(&mut lhs);
    right.hash(&mut rhs);
    assert_eq!(lhs.finish(), rhs.finish());
}

// A const-capable trait still permits an ordinary non-const impl and proof.
const trait ConstCapable {
    fn value(&self) -> u8;
}
impl ConstCapable for Unit {
    fn value(&self) -> u8 { 7 }
}
fn ordinary_const_trait<T: ConstCapable>(value: &T) -> u8 {
    value.value()
}

fn main() {
    let text = String::from("borrowed");
    check(&Name::Borrowed(&text), &Name::Borrowed(&text));
    check(&Name::Empty, &Name::Empty);
    check(&Unit, &Unit);
    assert_eq!(ordinary_const_trait(&Unit), 7);
}
