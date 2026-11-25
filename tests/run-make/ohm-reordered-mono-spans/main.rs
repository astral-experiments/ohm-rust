#![feature(rustc_attrs)]
#![allow(internal_features)]

#[inline(never)]
fn record<const DIGIT: u32>() {
    const { assert!(DIGIT != 1 || !cfg!(fail)) };
    std::hint::black_box(DIGIT);
}

#[inline(never)]
fn changed() {
    #[cfg(first)]
    {
        record::<1>();
        record::<2>();
    }

    #[cfg(not(first))]
    {
        record::<2>();
        record::<1>(); // The diagnostic must point here.
    }
}

#[rustc_clean(cfg = "second", loaded_from_disk = "incremental_mono_item_partitions")]
fn main() {
    changed();
}
