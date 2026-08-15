//@ revisions: bpass1 bpass2
//@ compile-flags: -Z query-dep-graph -Z public-api-hash
//@ ignore-backends: gcc

#![feature(rustc_attrs)]
#![crate_type = "rlib"]

pub fn first(value: usize) -> usize {
    #[cfg(bpass1)]
    {
        value + 1
    }

    #[cfg(bpass2)]
    {
        value + 3
    }
}

pub fn second(value: usize) -> usize {
    #[cfg(bpass1)]
    {
        value + 2
    }

    #[cfg(bpass2)]
    {
        value + 4
    }
}

#[rustc_clean(cfg = "bpass2", loaded_from_disk = "incremental_mono_item_partitions")]
pub fn unchanged() -> usize {
    first(4) + second(6)
}
