//@ revisions: bpass1 bpass2
//@ compile-flags: -Zquery-dep-graph
#![feature(rustc_attrs)]
#![allow(internal_features)]
#![crate_type = "rlib"]

#[cfg_attr(bpass2, derive(Clone))]
pub struct Unrelated;

fn require_clone<T: Clone>(_: T) {}

#[rustc_clean(cfg = "bpass2", loaded_from_disk = "typeck_root")]
pub fn capture(value: u64) {
    require_clone(move || value);
    require_clone(capture);
}
