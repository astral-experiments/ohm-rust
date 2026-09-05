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

impl<'a, T: PointeeSized> Copy for &'a T where 'a: 'static {}
