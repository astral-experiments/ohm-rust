//@ check-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![feature(no_core, lang_items)]
#![no_core]
#![crate_type = "lib"]
#![allow(internal_features)]

#[lang = "pointee_sized"]
pub trait PointeeSized {}
#[lang = "meta_sized"]
pub trait MetaSized: PointeeSized {}
#[lang = "sized"]
pub trait Sized: MetaSized {}

pub fn unit() {}
pub fn accept(_: (), _: ()) {}

pub fn call() {
    accept(unit(), ());
}
