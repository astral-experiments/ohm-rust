//@ aux-build: copy_impl_schema_specialized_core.rs
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![feature(no_core)]
#![no_core]
#![crate_type = "lib"]

extern crate copy_impl_schema_specialized_core as custom_core;

pub fn specialized(value: &u8) -> (&u8, &u8) {
    (value, value)
}

pub fn require_copy<T: custom_core::Copy>(_: T) {}

pub fn unspecialized(value: &u16) {
    require_copy(value);
    //~^ ERROR the trait bound
}
