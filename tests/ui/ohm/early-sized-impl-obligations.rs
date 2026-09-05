//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths

#![feature(sized_hierarchy)]

use std::marker::MetaSized;

trait Project {
    type Output: ?Sized;
}

impl Project for u8 {
    type Output = str;
}

struct Wrapped<T>(T);
trait Check {}

impl<T: Project> Check for Wrapped<T> where for<'a> &'a T::Output: Sized {}

fn require<T: Check>(_: T) {}
fn generic<T: Project>(value: T) { require(Wrapped(value)); }

trait MetaCheck {}
impl<T: MetaSized> MetaCheck for *const T {}

fn require_meta<T: MetaCheck>(_: T) {}

fn main() {
    // Instantiated implicit Sized bounds can contain freshly unified type variables.
    require(Wrapped(1u8));
    // Retain higher-ranked projection normalization, even when a reference is Sized.
    generic(2u8);
    // MetaSized permits types whose values are not Sized.
    require_meta("value" as *const str);
    require_meta(&[1u8, 2][..] as *const [u8]);
}
