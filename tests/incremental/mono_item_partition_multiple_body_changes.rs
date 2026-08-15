//@ revisions: rpass1 rpass2
//@ compile-flags: -Z query-dep-graph -Z public-api-hash
//@ [rpass1]exec-env:EXPECTED=24
//@ [rpass2]exec-env:EXPECTED=30
//@ ignore-backends: gcc

#![feature(rustc_attrs)]

fn first(value: usize) -> usize {
    #[cfg(rpass1)]
    {
        value + 1
    }

    #[cfg(rpass2)]
    {
        value + 3
    }
}

fn second(value: usize) -> usize {
    #[cfg(rpass1)]
    {
        value + 2
    }

    #[cfg(rpass2)]
    {
        value + 4
    }
}

fn third(value: usize) -> usize {
    #[cfg(rpass1)]
    {
        value + 3
    }

    #[cfg(rpass2)]
    {
        value + 5
    }
}

#[rustc_clean(cfg = "rpass2", loaded_from_disk = "incremental_mono_item_partitions")]
pub fn unchanged() -> usize {
    first(4) + second(6) + third(8)
}

fn main() {
    assert_eq!(unchanged().to_string(), std::env::var("EXPECTED").expect("EXPECTED must be set"));
}
