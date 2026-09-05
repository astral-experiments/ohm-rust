//@ aux-build: copy_impl_schema_lifetime_core.rs
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![feature(no_core)]
#![no_core]
#![crate_type = "lib"]

extern crate copy_impl_schema_lifetime_core as custom_core;

pub fn duplicate<'a>(value: &'a u8) -> (&'a u8, &'a u8) {
    (value, value)
    //~^ ERROR lifetime may not live long enough
}
