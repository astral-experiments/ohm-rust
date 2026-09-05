//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths

#![feature(no_core, lang_items)]
#![no_core]
#![allow(internal_features)]

#[lang = "pointee_sized"]
trait PointeeSized {}
#[lang = "meta_sized"]
trait MetaSized: PointeeSized {}
#[lang = "sized"]
trait Sized: MetaSized {}
#[lang = "copy"]
trait Copy {}

// Even shared references can have a conditional Copy impl with custom lang items.
impl Copy for &'static u8 {}

fn conditional<'a>(value: &'a u8) -> (&'a u8, &'a u8) {
    (value, value)
    //~^ ERROR lifetime may not live long enough
}

fn main() {}
