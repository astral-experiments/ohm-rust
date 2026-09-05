//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![feature(inherent_associated_types)]
#![allow(dead_code, incomplete_features)]

trait Ordinary {}
#[derive(Clone, Copy)]
struct Token;
impl Ordinary for Token {}
const fn require<T: Ordinary>() -> usize { 7 }

// These roots use ordinary fulfillment, even though the impl is eligible.
const STANDALONE: usize = require::<Token>();
static STATIC: usize = require::<Token>();
struct Owner;
impl Owner {
    const ASSOCIATED: usize = require::<Token>();
}
const ARRAY: [(); require::<Token>()] = [(); 7];

// Building the trait's impl buckets also visits other impl headers.
struct Aliased;
type Alias = Aliased;
impl Ordinary for Alias {}

trait Project { type Item; }
struct Family;
struct Projected;
impl Project for Family { type Item = Projected; }
impl Ordinary for <Family as Project>::Item {}

struct Inherent;
impl Owner { type Item = Inherent; }
impl Ordinary for Owner::Item {}

fn main() {
    assert_eq!(require::<Token>(), 7);
    assert_eq!(require::<Aliased>(), 7);
    assert_eq!(require::<Projected>(), 7);
    assert_eq!(require::<Owner::Item>(), 7);
    assert_eq!(STANDALONE, 7);
    assert_eq!(STATIC, 7);
    assert_eq!(Owner::ASSOCIATED, 7);
    assert_eq!(ARRAY.len(), 7);
}
