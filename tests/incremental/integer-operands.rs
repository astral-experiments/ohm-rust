//@ revisions: bpass1 bpass2 bfail3 bfail4
//@ compile-flags: -Zquery-dep-graph -Zthreads=1
#![crate_type = "rlib"]
#![feature(rustc_attrs)]
#![allow(internal_features)]

pub struct Other;

#[cfg(not(bpass1))]
impl std::ops::Add<Other> for u32 {
    type Output = bool;
    fn add(self, _: Other) -> bool {
        true
    }
}

#[cfg(not(bfail3))]
type Right = u32;
#[cfg(bfail3)]
type Right = Other;
#[cfg(not(bfail4))]
type Flag = bool;
#[cfg(bfail4)]
type Flag = usize;

// Both a known integer and an integer literal probe Add before the RHS has
// been checked. An impl for a different RHS leaves the answers unchanged.
#[rustc_clean(cfg = "bpass2", loaded_from_disk = "typeck_root")]
pub fn sum<const F: Flag>(left: u32, right: Right) -> u32 {
    if F {
        //[bfail4]~^ ERROR mismatched types
        left + right
        //[bfail3]~^ ERROR mismatched types
    } else {
        0 + left
    }
}
