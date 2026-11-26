//@ revisions: rpass1 rpass2 rpass3 rpass4
//@ aux-build: source.rs
//@ no-prefer-dynamic
//@ compile-flags: -Zquery-dep-graph -Zshare-generics=yes -Ccodegen-units=16 -Zincremental-verify-ich
//@ ignore-backends: gcc

// A private item moves metadata for the same ordered list of upstream
// instances. Request another instance after each move, including the revert.
#![feature(rustc_attrs)]
#![allow(internal_features)]
#![cfg_attr(rpass2, rustc_partition_reused(module = "main-stable", cfg = "rpass2"))]
#![cfg_attr(rpass3, rustc_partition_reused(module = "main-stable", cfg = "rpass3"))]
#![cfg_attr(rpass4, rustc_partition_reused(module = "main-stable", cfg = "rpass4"))]

extern crate source;

mod stable {
    #[inline(never)]
    pub fn get() -> u32 {
        source::number::<[u32; 3], 0>()
    }
}

fn main() {
    assert_eq!(stable::get(), 0);
    #[cfg(rpass2)]
    assert_eq!(source::number::<[u32; 3], 177>(), 177);
    #[cfg(rpass3)]
    assert_eq!(source::number::<[u32; 3], 231>(), 231);
    // The provider is unchanged in this revision. Verify a first lookup
    // against the flat decoder even if the exported symbol query is green.
    #[cfg(rpass4)]
    assert_eq!(source::number::<[u32; 3], 183>(), 183);
}
