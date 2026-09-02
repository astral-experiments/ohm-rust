//@ check-pass
//@ edition:2021
//@ compile-flags: --crate-type=lib
//@ aux-build:async-drop-metadata-aux.rs

#![feature(async_drop, type_alias_impl_trait)]
#![allow(dead_code, incomplete_features)]

extern crate async_drop_metadata_aux;

use std::future::{async_drop_in_place, Future};

type Dropping = impl Future<Output = ()>;

#[define_opaque(Dropping)]
fn dropping(value: &'static mut async_drop_metadata_aux::Payload) -> Dropping {
    unsafe { async_drop_in_place(value) }
}

// The instantiated async-drop layout includes the external destructor future's
// layout, which must be available even when the auxiliary emits only metadata.
static mut CHECK: Option<Dropping> = None;
