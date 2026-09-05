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

pub trait Required {}
#[lang = "copy"]
pub trait Copy: Required {}

pub struct MissingSupertrait;
impl Copy for MissingSupertrait {}
//~^ ERROR the trait bound `MissingSupertrait: Required` is not satisfied

pub fn duplicate(value: MissingSupertrait) -> (MissingSupertrait, MissingSupertrait) {
    (value, value)
}
