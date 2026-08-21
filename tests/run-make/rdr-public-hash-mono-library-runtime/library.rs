#![feature(rustc_attrs)]
#![crate_type = "rlib"]

use std::sync::atomic::{AtomicUsize, Ordering};

static CALLS: AtomicUsize = AtomicUsize::new(0);

#[rustc_clean(cfg = "changed", loaded_from_disk = "incremental_mono_item_partitions")]
pub fn value() -> usize {
    evaluate()
}

fn evaluate() -> usize {
    CALLS.store(0, Ordering::Relaxed);

    [1_u8].iter().any(|value| {
        #[cfg(not(changed))]
        {
            equal(*value, 1) && equal(2, *value)
        }

        #[cfg(changed)]
        {
            equal(2, *value) && equal(*value, 1)
        }
    });

    CALLS.load(Ordering::Relaxed)
}

#[inline(never)]
fn equal<T: PartialEq>(left: T, right: T) -> bool {
    CALLS.fetch_add(1, Ordering::Relaxed);
    left == right
}
