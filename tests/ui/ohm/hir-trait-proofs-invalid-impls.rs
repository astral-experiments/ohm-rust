//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

#[derive(Clone)]
struct InvalidCopy(String);
impl Copy for InvalidCopy {}
//~^ ERROR the trait `Copy` cannot be implemented for this type

trait Super {}
trait Ordinary: Super {}
struct MissingSupertrait;
impl Ordinary for MissingSupertrait {}
//~^ ERROR the trait bound `MissingSupertrait: Super` is not satisfied

trait Coherent {}
struct Duplicate;
impl Coherent for Duplicate {}
impl Coherent for Duplicate {}
//~^ ERROR conflicting implementations of trait `Coherent` for type `Duplicate`

fn require<T: Ordinary>() {}
fn copy<T: Copy>(_: T) {}
fn coherent<T: Coherent>() {}

fn main() {
    require::<MissingSupertrait>();
    copy(InvalidCopy(String::new()));
    coherent::<Duplicate>();
}
