//@ run-pass
//@ needs-unwind
//@ edition: 2024

// Two invocations share the bridge counter but must only retrieve spans from their own store.
#[path = "../../../library/proc_macro/src/bridge/fxhash.rs"]
mod fxhash;
#[allow(dead_code)]
#[path = "../../../library/proc_macro/src/bridge/handle.rs"]
mod handle;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU32, Ordering};

use handle::InternedStore;

fn main() {
    static COUNTER: AtomicU32 = AtomicU32::new(10);
    let mut spans = InternedStore::new(&COUNTER);
    let mut other = InternedStore::new(&COUNTER);
    let first = spans.alloc(1);
    let foreign = other.alloc(2);
    let sparse = spans.alloc(3);

    assert_eq!(spans.alloc(1), first);
    assert_eq!(spans.copy(first), 1);
    assert_eq!(spans.copy(sparse), 3);
    assert!(catch_unwind(AssertUnwindSafe(|| spans.copy(foreign))).is_err());

    // Simulate another invocation exhausting the shared counter while these spans are live.
    COUNTER.store(u32::MAX, Ordering::Relaxed);
    other.alloc(4);
    assert!(catch_unwind(AssertUnwindSafe(|| other.alloc(5))).is_err());
    COUNTER.fetch_add(10, Ordering::Relaxed);
    spans.alloc(6);
    assert!(catch_unwind(AssertUnwindSafe(|| spans.alloc(7))).is_err());
}
