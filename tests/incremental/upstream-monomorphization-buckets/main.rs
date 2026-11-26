//@ revisions: rpass1 rpass2 rpass3 rpass4
//@[rpass2] compile-flags: --cfg reuse
//@[rpass3] compile-flags: --cfg reuse
//@[rpass4] compile-flags: --cfg reuse
//@ aux-build: provider.rs
//@ no-prefer-dynamic
//@ compile-flags: -Zquery-dep-graph -Zshare-generics=yes -Ccodegen-units=16
//@ ignore-backends: gcc

// The provider changes its u16 instantiation to u32 and back. The u8 instance stays
// upstream, and the array instance stays local to the consumer.
#![feature(rustc_attrs)]
#![allow(internal_features)]
#![rustc_partition_reused(module = "main-present", cfg = "reuse")]
#![rustc_partition_reused(module = "main-absent", cfg = "reuse")]

extern crate provider;

mod present {
    #[inline(never)]
    pub fn get() -> usize {
        provider::width::<u8>()
    }
}

mod absent {
    #[inline(never)]
    pub fn get() -> usize {
        provider::width::<[u8; 37]>()
    }
}

fn main() {
    // The upstream instances stay the same as rpass3. Look up a new absent one.
    #[cfg(rpass4)]
    assert_eq!(provider::width::<[u8; 41]>(), 41);
    assert_eq!((present::get(), absent::get()), (1, 37));
    assert_eq!((provider::width::<u16>(), provider::width::<u32>()), (2, 4));
    assert_eq!(provider::widths(), (1, if cfg!(rpass2) { 4 } else { 2 }));
}
