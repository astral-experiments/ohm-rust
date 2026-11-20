//@ revisions: rpass1 rpass2
//@ aux-build: provider.rs
//@ no-prefer-dynamic
//@ compile-flags: -Zquery-dep-graph -Zshare-generics=yes -Ccodegen-units=16
//@ ignore-backends: gcc

// The provider changes its u16 instantiation to u32. The u8 instance stays
// upstream, and the array instance stays local to the consumer.
#![feature(rustc_attrs)]
#![allow(internal_features)]
#![rustc_partition_reused(module = "main-present", cfg = "rpass2")]
#![rustc_partition_reused(module = "main-absent", cfg = "rpass2")]

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
    assert_eq!((present::get(), absent::get()), (1, 37));
    assert_eq!((provider::width::<u16>(), provider::width::<u32>()), (2, 4));
    assert_eq!(provider::widths(), (1, if cfg!(rpass1) { 2 } else { 4 }));
}
