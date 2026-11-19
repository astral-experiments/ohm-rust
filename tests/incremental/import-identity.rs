//@ revisions: bpass1 bpass2 bfail3
//@ compile-flags: -Zquery-dep-graph

#![feature(rustc_attrs)]
#![allow(internal_features, unused_imports)]
#![crate_type = "lib"]

// An in-scope trait candidate and the type check that selects it both record
// the import's definition ID. Removing a sibling should preserve that ID.
// Rebinding the same name to a different trait should redo method lookup.

pub mod traits {
    pub trait Extension {
        fn extension(&self);
    }
    impl Extension for () {
        fn extension(&self) {}
    }
    pub trait Before {}
    pub trait Spare {}
}

pub mod replacement {
    pub trait Extension {
        fn extension(&self) -> u8;
    }
    impl Extension for () {
        fn extension(&self) -> u8 {
            0
        }
    }
}

pub mod siblings {
    #[cfg(bpass1)]
    use crate::traits::Before;
    #[cfg(any(bpass1, bpass2))]
    use crate::traits::Extension;
    #[cfg(bfail3)]
    use crate::replacement::Extension;

    #[rustc_clean(cfg = "bpass2")]
    pub fn call() {
        ().extension() //[bfail3]~ ERROR mismatched types
    }
}

pub mod nested {
    #[cfg(bpass1)]
    use crate::traits::{Before, Extension, Spare};
    #[cfg(any(bpass2, bfail3))]
    use crate::traits::{Extension, Spare};

    #[rustc_clean(cfg = "bpass2")]
    pub fn call() {
        ().extension();
    }
}
