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

pub trait Gate: PointeeSized {}
pub struct NoGate;

impl<T: PointeeSized + Gate> Copy for &T {}
impl<T: PointeeSized + Gate> Copy for *const T {}
impl<T: PointeeSized + Gate> Copy for *mut T {}
