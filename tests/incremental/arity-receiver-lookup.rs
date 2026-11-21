//@ revisions: bpass1 bpass2 bfail3 bfail4 bfail5
//@ compile-flags: -Zquery-dep-graph
#![crate_type = "rlib"]
#![feature(rustc_attrs)]
#![allow(internal_features)]

pub trait Accept {}

#[cfg_attr(not(bpass1), derive(Copy, Clone))]
pub struct Unrelated;

#[cfg(not(any(bfail3, bfail5)))]
impl Accept for fn(u8) {}
#[cfg(bfail3)]
impl Accept for fn(u16) {}
#[cfg(bfail5)]
impl Accept for fn(u8, u8) {}

#[cfg(not(bfail4))]
impl Accept for (u8, u8, u8, u8, u8, u8) {}
#[cfg(bfail4)]
impl Accept for (u8, u8, u8, u8, u8) {}

fn require<T: Copy + Accept>(_: T) {}

// Adding an unrelated impl leaves both receiver buckets unchanged.
// A changed signature matters even when its arity stays the same.
#[rustc_clean(cfg = "bpass2", loaded_from_disk = "typeck_root")]
pub fn pointer(x: fn(u8)) {
    require(x);
    //[bfail3,bfail5]~^ ERROR the trait bound
}

#[rustc_clean(cfg = "bpass2", loaded_from_disk = "typeck_root")]
pub fn tuple(x: (u8, u8, u8, u8, u8, u8)) {
    require(x);
    //[bfail4]~^ ERROR the trait bound
}
