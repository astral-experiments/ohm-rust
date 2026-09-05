#![feature(no_core)]
#![no_core]
extern crate custom_core;
pub fn duplicate<'a, T: custom_core::PointeeSized>(value: &'a T) -> (&'a T, &'a T) {
    (value, value)
}
