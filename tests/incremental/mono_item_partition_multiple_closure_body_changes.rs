//@ revisions: bpass1 bpass2
//@ compile-flags: -Z query-dep-graph -Z public-api-hash
//@ ignore-backends: gcc

#![feature(rustc_attrs)]
#![crate_type = "rlib"]

fn first(values: &[u8]) -> bool {
    values.iter().any(|value| {
        #[cfg(bpass1)]
        {
            *value > 1
        }

        #[cfg(bpass2)]
        {
            *value < 1
        }
    })
}

fn second(values: &[u8]) -> bool {
    values.iter().any(|value| {
        #[cfg(bpass1)]
        {
            *value > 2
        }

        #[cfg(bpass2)]
        {
            *value < 2
        }
    })
}

#[rustc_clean(cfg = "bpass2", loaded_from_disk = "incremental_mono_item_partitions")]
pub fn unchanged() -> bool {
    first(&[1, 2]) || second(&[2, 3])
}
