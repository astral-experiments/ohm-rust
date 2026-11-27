//@ revisions: bpass1 bpass2 bfail3
//@ compile-flags: -Zquery-dep-graph

#![crate_type = "lib"]
#![feature(rustc_attrs)]
#![allow(internal_features)]

// Changing the length of an associated item's docs should let report reuse
// type checking. Removing the bound on the associated type must invalidate it.
#[cfg(bpass1)]
pub trait Sink {
    /// Error.
    type Error: Copy;
}

#[cfg(bpass2)]
pub trait Sink {
    /// An error returned by the sink.
    type Error: Copy;
}

#[cfg(bfail3)]
pub trait Sink {
    type Error;
}

fn require_copy<T: Copy>(_: T) {}

#[rustc_clean(cfg = "bpass2", loaded_from_disk = "typeck_root")]
pub fn report<S: Sink>(err: S::Error) {
    require_copy::<S::Error>(err);
    //[bfail3]~^ ERROR the trait bound
}
