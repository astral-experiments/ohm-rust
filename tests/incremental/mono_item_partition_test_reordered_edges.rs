//@ revisions: rpass1 rpass2 rpass3
//@ compile-flags: --test -Zquery-dep-graph -Zpublic-api-hash=no -Zohm-incremental-mono-partitions -Zincremental-verify-ich
//@ [rpass1]exec-env:EXPECTED=12
//@ [rpass2]exec-env:EXPECTED=21
//@ [rpass3]exec-env:EXPECTED=12
//@ ignore-backends: gcc

#![feature(rustc_attrs)]

use std::sync::atomic::{AtomicUsize, Ordering};

static CALL_ORDER: AtomicUsize = AtomicUsize::new(0);

#[inline(never)]
fn record<const DIGIT: usize>() -> bool {
    let previous = CALL_ORDER.load(Ordering::Relaxed);
    CALL_ORDER.store(previous * 10 + DIGIT, Ordering::Relaxed);
    true
}

fn changed() -> bool {
    #[cfg(not(rpass2))]
    {
        record::<1>() && record::<2>()
    }

    #[cfg(rpass2)]
    {
        record::<2>() && record::<1>()
    }
}

#[rustc_clean(cfg = "rpass2", loaded_from_disk = "incremental_mono_item_partitions")]
#[rustc_clean(cfg = "rpass3", loaded_from_disk = "incremental_mono_item_partitions")]
fn unchanged() -> bool {
    changed()
}

#[test]
fn reused_partition_preserves_changed_call_order() {
    CALL_ORDER.store(0, Ordering::Relaxed);
    assert!(unchanged());
    assert_eq!(
        CALL_ORDER.load(Ordering::Relaxed).to_string(),
        std::env::var("EXPECTED").expect("EXPECTED must be set"),
    );
}
