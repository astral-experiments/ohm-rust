// An unchanged derive can leave the owner's ordinary queries green.

//@ proc-macro:derive_nothing.rs
//@ revisions:rpass1 rpass2
//@ compile-flags: -Zquery-dep-graph -Zcache-proc-macros
//@ ignore-backends: gcc

#![feature(rustc_attrs)]

#[macro_use]
extern crate derive_nothing;

#[cfg(any(rpass1, rpass2))]
#[rustc_clean(cfg = "rpass2")]
#[derive(Nothing)]
pub struct Foo;

fn main() {}
