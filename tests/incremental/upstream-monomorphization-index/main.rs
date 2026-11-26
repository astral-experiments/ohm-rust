//@ revisions: rpass1 rpass2 rpass3 rpass4
//@[rpass2] compile-flags: --cfg reuse
//@[rpass3] compile-flags: --cfg reuse
//@[rpass4] compile-flags: --cfg reuse
//@ aux-build: provider.rs
//@ no-prefer-dynamic
//@ compile-flags: -Zquery-dep-graph -Zshare-generics=yes -Ccodegen-units=16
//@ ignore-backends: gcc

// Request an already exported instance for the first time, then withdraw
// and restore its export. Unaffected instantiations must remain reusable.
#![feature(rustc_attrs)]
#![allow(internal_features)]
#![rustc_partition_reused(module = "main-present", cfg = "reuse")]

extern crate provider;

mod present {
    #[inline(never)]
    pub fn get() -> usize {
        provider::width::<u8>()
    }
}

fn main() {
    assert_eq!(present::get(), 1);
    assert_eq!(provider::widths(), (1, if cfg!(rpass3) { 4 } else { 8 }));
    #[cfg(not(rpass1))]
    assert_eq!((provider::width::<u64>(), provider::width::<u32>()), (8, 4));
}
