//@ revisions: bpass1 bfail2
//@ compile-flags: -Z query-dep-graph -Z public-api-hash
//@ ignore-backends: gcc

#![feature(rustc_attrs)]
#![crate_type = "rlib"]

pub fn first() -> bool {
    #[cfg(bpass1)]
    {
        equal(1_u8, 1_u8)
    }

    #[cfg(bfail2)]
    {
        equal(1_u16, 1_u16)
    }
}

pub fn second(value: usize) -> usize {
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
    usize::from(first()) + second(1)
}

#[inline(never)]
fn equal<T: PartialEq>(left: T, right: T) -> bool {
    left == right
}
