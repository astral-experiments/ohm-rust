//@ revisions: bpass1 bpass2 bfail3
//@ compile-flags: -Zquery-dep-graph
//@ aux-build: scoped_unreachable_macro.rs

#![feature(rustc_attrs)]
#![allow(internal_features)]
#![deny(unreachable_code)]
#![crate_type = "rlib"]

extern crate scoped_unreachable_macro;

pub struct Local;

impl Local {
    #[rustc_clean(cfg = "bpass2", loaded_from_disk = "typeck_root")]
    pub fn locally_allowed() {
        #[allow(unreachable_code)]
        {
            return;
            return;
        }
    }

    pub fn allowed_by_foreign_macro() {
        scoped_unreachable_macro::with_allow!({
            return;
            return;
        });
    }

    // Adding a sibling changes the impl without changing either method body.
    #[cfg(bpass2)]
    pub fn added() {}

    #[cfg(bfail3)]
    pub fn reported() {
        return;
        return;
        //[bfail3]~^ ERROR unreachable statement
    }
}
