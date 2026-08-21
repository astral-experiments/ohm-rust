//@ revisions: bpass1 bfail2
//@ compile-flags: -Z query-dep-graph -Z public-api-hash
//@ ignore-backends: gcc

#![feature(rustc_attrs)]
#![crate_type = "rlib"]

#[rustc_clean(cfg = "bfail2", loaded_from_disk = "incremental_mono_item_partitions")]
pub fn unchanged() -> bool {
    //[bfail2]~^ ERROR should have been loaded from disk but it was not
    changed()
}

fn changed() -> bool {
    [1_u8].iter().any(|value| {
        #[cfg(bpass1)]
        {
            equal(*value, 1_u8) && equal(2_u16, 2_u16)
        }

        #[cfg(bfail2)]
        {
            equal(2_u16, 2_u16) && equal(*value, 1_u8)
        }
    })
}

#[inline(never)]
fn equal<T: PartialEq>(left: T, right: T) -> bool {
    left == right
}
