//@ aux-build: copy_impl_schema_pointee_core.rs
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![feature(no_core)]
#![no_core]
#![crate_type = "lib"]

extern crate copy_impl_schema_pointee_core as custom_core;

use custom_core::NoGate;

pub fn require_copy<T: custom_core::Copy>(_: T) {}

pub fn shared(value: &NoGate) {
    require_copy(value);
    //~^ ERROR the trait bound
}
