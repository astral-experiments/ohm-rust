//@ revisions: bpass1 bpass2 bfail3
//@ compile-flags: -Zquery-dep-graph -Zincremental-ignore-spans -Zthreads=1

#![feature(decl_macro, rustc_attrs)]
#![allow(internal_features)]
#![crate_type = "lib"]

pub struct Receiver;

// Add a method with the same name in a different syntax context. Only the
// caller inside the macro should see it, even when rustc ignores spans.
macro methods($public_name:ident) {
    impl Receiver {
        #[cfg(any(bpass1, bpass2))]
        pub fn $public_name(&self) -> u8 {
            1
        }

        #[cfg(any(bpass2, bfail3))]
        pub fn value(&self) -> bool {
            true
        }
    }

    // The two identifiers in the nested macro share their outer expansion
    // mark but have different parent contexts.
    macro callers() {
        pub fn inner(x: &Receiver) -> bool {
            let _: u8 = x.$public_name();
            x.value()
        }
    }
    #[cfg(bpass2)]
    callers!();
}

methods!(value);

pub trait Fallback {
    fn value(&self);
}

impl Fallback for Receiver {
    fn value(&self) {}
}

// The first caller repeats the lookup after the impl changes. The second
// can reuse its type check if the lookup still selects the same method.
pub fn first(x: &Receiver) {
    x.value();
}

#[rustc_clean(cfg = "bpass2", loaded_from_disk = "typeck_root")]
pub fn second(x: &Receiver) -> u8 {
    x.value() //[bfail3]~ ERROR mismatched types
}
