//@ revisions: bpass1 bpass2
//@ compile-flags: --emit=dep-info,link -Zquery-dep-graph -Zpublic-api-hash -Zohm-incremental-mono-partitions -Zohm-check-relink-inputs -Zincremental-verify-ich
//@ ignore-backends: gcc

#![feature(rustc_attrs)]
#![crate_type = "rlib"]

// Both exported functions are collection roots. Changing their bodies preserves their
// monomorphization edges and size estimates.
pub fn first() -> u8 {
    if cfg!(bpass1) { 1 } else { 2 }
}

pub fn second() -> u8 {
    if cfg!(bpass1) { 2 } else { 1 }
}

#[rustc_clean(cfg = "bpass2", loaded_from_disk = "incremental_mono_item_partitions")]
pub fn sum() -> u8 {
    first() + second()
}
