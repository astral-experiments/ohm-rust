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

#[lang = "copy"]
pub trait Copy {}
impl<T: PointeeSized> Copy for &T {}

pub struct Empty;
impl Copy for Empty {}

pub struct Node<'a>(&'a Node<'a>);
impl<'a> Copy for Node<'a> {}

pub fn duplicate_empty(value: Empty) -> (Empty, Empty) {
    (value, value)
}

pub fn duplicate_node<'a>(value: Node<'a>) -> (Node<'a>, Node<'a>) {
    (value, value)
}
