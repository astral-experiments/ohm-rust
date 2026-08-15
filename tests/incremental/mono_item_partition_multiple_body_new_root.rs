//@ revisions: bpass1 bfail2
//@ compile-flags: -Z query-dep-graph -Z public-api-hash
//@ ignore-backends: gcc

#![feature(rustc_attrs)]
#![crate_type = "rlib"]

fn first(value: usize) -> usize {
    #[cfg(bpass1)]
    {
        value + 1
    }

    #[cfg(bfail2)]
    {
        value + 2
    }
}

fn second(value: usize) -> usize {
    #[cfg(bpass1)]
    {
        value + 1
    }

    #[cfg(bfail2)]
    {
        value + 2
    }
}

#[cfg(bfail2)]
pub fn newly_exported(value: usize) -> usize {
    value
}

#[rustc_clean(cfg = "bfail2", loaded_from_disk = "incremental_mono_item_partitions")]
pub fn unchanged() -> usize {
    //[bfail2]~^ ERROR should have been loaded from disk but it was not
    first(1) + second(2)
}
