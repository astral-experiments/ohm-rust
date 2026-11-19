//@ revisions: bpass1 bfail2
//@ compile-flags: -Zquery-dep-graph

#![feature(rustc_attrs)]
#![allow(internal_features)]
#![warn(dead_code)]
#![crate_type = "lib"]

#[cfg(bfail2)]
pub fn entry() {
    update(0);
}

// The unused-import pass requests this body's uses in both revisions.
// The dead-code pass only reaches the assignment in the second revision.
#[rustc_clean(cfg = "bfail2", loaded_from_disk = "body_uses")]
fn update(mut x: u32) -> u32 {
    //[bpass1]~^ WARN function `update` is never used
    #[deny(dead_code)]
    {
        x = x;
        //[bfail2]~^ ERROR useless assignment of variable
    }
    x
}
