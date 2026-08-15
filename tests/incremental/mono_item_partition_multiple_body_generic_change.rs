//@ revisions: bpass1 bfail2
//@ compile-flags: -Z query-dep-graph -Z public-api-hash
//@ ignore-backends: gcc

#![feature(rustc_attrs)]
#![crate_type = "rlib"]

fn generic<T: Into<usize>>(value: T) -> usize {
    #[cfg(bpass1)]
    {
        value.into() + 1
    }

    #[cfg(bfail2)]
    {
        value.into() + 2
    }
}

fn nongeneric(value: usize) -> usize {
    #[cfg(bpass1)]
    {
        value + 1
    }

    #[cfg(bfail2)]
    {
        value + 2
    }
}

#[rustc_clean(cfg = "bfail2", loaded_from_disk = "incremental_mono_item_partitions")]
pub fn unchanged() -> usize {
    //[bfail2]~^ ERROR should have been loaded from disk but it was not
    generic(1_u8) + nongeneric(1)
}
