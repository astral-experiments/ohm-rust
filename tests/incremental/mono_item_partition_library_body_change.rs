//@ revisions: bpass1 bpass2
//@ compile-flags: -Z query-dep-graph -Z public-api-hash
//@ ignore-backends: gcc

#![feature(rustc_attrs)]
#![crate_type = "rlib"]

mod changed {
    pub fn evaluate(values: &[u8]) -> bool {
        values.iter().any(|value| {
            #[cfg(bpass1)]
            {
                equal(*value, 1) && equal(2, *value)
            }

            #[cfg(bpass2)]
            {
                equal(2, *value) && equal(*value, 1)
            }
        })
    }

    #[inline(never)]
    fn equal<T: PartialEq>(left: T, right: T) -> bool {
        left == right
    }
}

#[rustc_clean(cfg = "bpass2", loaded_from_disk = "incremental_mono_item_partitions")]
pub fn unchanged() -> bool {
    changed::evaluate(&[1, 2])
}
