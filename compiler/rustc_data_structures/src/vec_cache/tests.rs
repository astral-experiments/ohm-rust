use super::*;

#[test]
#[should_panic(expected = "bucket index out of range")]
fn bucket_index_n_buckets() {
    BucketIndex::from_raw(BUCKETS);
}

#[test]
fn bucket_index_round_trip() {
    for i in 0..BUCKETS {
        assert_eq!(BucketIndex::from_raw(i).to_usize(), i);
    }
}

#[test]
fn bucket_index_iter_all_len() {
    let len = BucketIndex::iter_all().len();
    assert_eq!(len, BUCKETS);

    let len = BucketIndex::iter_all().collect::<Vec<_>>().len();
    assert_eq!(len, BUCKETS);

    let len = BucketIndex::enumerate_buckets(&[(); BUCKETS]).len();
    assert_eq!(len, BUCKETS);
}

#[test]
fn bucket_index_capacity() {
    // Check that the combined capacity of all buckets is 2^32 slots.
    // That's 1 larger than `u32::MAX`, so store the total as a `u64`.
    let mut total = 0u64;
    for i in BucketIndex::iter_all() {
        total += u64::try_from(i.capacity()).unwrap();
    }
    assert_eq!(total, 1 << 32);
}

#[test]
#[cfg(not(miri))]
fn vec_cache_empty_exhaustive() {
    let cache: VecCache<u32, u32, u32> = VecCache::default();
    for key in 0..=u32::MAX {
        assert!(cache.lookup(&key).is_none());
    }
}

#[test]
fn vec_cache_insert_and_check() {
    let cache: VecCache<u32, u32, u32> = VecCache::default();
    cache.complete(0, 1, 2);
    assert_eq!(cache.lookup(&0), Some((1, 2)));
}

#[test]
fn sparse_inserts() {
    let cache: VecCache<u32, u8, u32> = VecCache::default();
    let end = if cfg!(target_pointer_width = "64") && cfg!(target_os = "linux") {
        // For paged memory, 64-bit systems we should be able to sparsely allocate all of the pages
        // needed for these inserts cheaply (without needing to actually have gigabytes of resident
        // memory).
        31
    } else {
        // Otherwise, still run the test but scaled back:
        //
        // Each slot is 5 bytes, so 2^25 entries (on non-virtual memory systems, like e.g. Windows) will
        // mean 160 megabytes of allocated memory. Going beyond that is probably not reasonable for
        // tests.
        25
    };
    for shift in 0..end {
        let key = 1u32 << shift;
        cache.complete(key, shift, key);
        assert_eq!(cache.lookup(&key), Some((shift, key)));
    }
}

#[test]
fn concurrent_stress_check() {
    fn value(key: u32) -> [u32; 4] {
        [key, !key, key + 1, key + 2]
    }

    crate::sync::set_dyn_thread_safe_mode(true);
    let proof = crate::sync::check_dyn_thread_safe().unwrap();
    let cache: VecCache<u32, [u32; 4], u32> = VecCache::default();
    assert!(matches!(cache.mode, Mode::Sync));
    let barrier = std::sync::Barrier::new(100);
    std::thread::scope(|s| {
        for idx in 0..100 {
            let cache = proof.derive(&cache);
            let barrier = &barrier;
            s.spawn(move || {
                barrier.wait();
                cache.complete(idx, value(idx), idx);
                // Read while other threads publish their entries, before joining them.
                for key in 0..100 {
                    if let Some(entry) = cache.lookup(&key) {
                        assert_eq!(entry, (value(key), key));
                    }
                }
            });
        }
    });

    for idx in 0..100 {
        assert_eq!(cache.lookup(&idx), Some((value(idx), idx)));
    }
    assert_eq!(cache.len(), 100);
    cache.for_each(&mut |key, actual, index| {
        assert_eq!((*actual, index), (value(*key), *key));
    });
}

#[test]
#[cfg(not(miri))]
fn modes_are_captured_at_construction() {
    const CHILD: &str = "RUSTC_VEC_CACHE_MODE_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // The runtime mode is process-global and other tests enable synchronization.
        // Use a fresh process to test both an unset mode and permanently disabled synchronization.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "vec_cache::tests::modes_are_captured_at_construction"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        return;
    }

    let before: VecCache<u32, u32, u32> = VecCache::default();
    assert!(matches!(before.mode, Mode::Sync));
    crate::sync::set_dyn_thread_safe_mode(false);
    assert!(matches!(before.mode, Mode::Sync));
    let after: VecCache<u32, u32, u32> = VecCache::default();
    assert!(matches!(after.mode, Mode::NoSync));
    for cache in [before, after] {
        for key in [0, 4096] {
            cache.complete(key, key + 1, key + 2);
            assert_eq!(cache.lookup(&key), Some((key + 1, key + 2)));
        }
        assert_eq!(cache.len(), 2);
        cache.for_each(&mut |key, value, index| assert_eq!((*value, index), (key + 1, key + 2)));
    }
}

#[test]
fn value_marker_controls_auto_traits() {
    macro_rules! assert_not_impl {
        ($ty:ty, $bound:path) => {
            const _: fn() = || {
                trait AmbiguousIfImpl<A> {
                    fn check() {}
                }
                impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
                impl<T: ?Sized + $bound> AmbiguousIfImpl<u8> for T {}
                let _ = <$ty as AmbiguousIfImpl<_>>::check;
            };
        };
    }
    fn assert_send<T: Send>() {}

    type Cache = VecCache<u32, u32, u32>;
    assert_send::<Cache>();
    crate::sync::assert_dyn_send::<Cache>();
    crate::sync::assert_dyn_sync::<Cache>();
    assert_not_impl!(Cache, Sync);

    type LocalValue = VecCache<u32, &'static std::cell::Cell<u32>, u32>;
    assert_not_impl!(LocalValue, Send);
    assert_not_impl!(LocalValue, DynSend);
    assert_not_impl!(LocalValue, DynSync);

    // Query caches store values which are dynamically safe but lack ordinary auto traits.
    type DynamicValue = VecCache<u32, &'static crate::sync::Lock<()>, u32>;
    crate::sync::assert_dyn_send::<DynamicValue>();
    crate::sync::assert_dyn_sync::<DynamicValue>();
    assert_not_impl!(DynamicValue, Send);
}

#[test]
fn dropping_cache_does_not_access_values() {
    // The declaration order drops the String before the cache. The value marker must preserve
    // the existing may_dangle contract, since dropping the cache only deallocates its buckets.
    let cache;
    let value = String::from("cached value");
    cache = VecCache::<u32, &str, u32>::default();
    cache.complete(0, value.as_str(), 1);
    assert_eq!(cache.lookup(&0), Some((value.as_str(), 1)));
}

#[test]
fn slot_entries_table() {
    assert_eq!(
        ENTRIES_BY_BUCKET,
        [
            4096, 4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576, 2097152,
            4194304, 8388608, 16777216, 33554432, 67108864, 134217728, 268435456, 536870912,
            1073741824, 2147483648
        ]
    );
}

#[test]
fn bucket_entries_matches() {
    for i in BucketIndex::iter_all() {
        assert_eq!(i.capacity(), ENTRIES_BY_BUCKET[i]);
    }
}

#[test]
#[cfg(not(miri))]
fn slot_index_exhaustive() {
    let mut buckets = [0u32; 21];
    for idx in 0..=u32::MAX {
        buckets[SlotIndex::from_index(idx).bucket_idx] += 1;
    }
    let slot_idx = SlotIndex::from_index(0);
    assert_eq!(slot_idx.index_in_bucket, 0);
    assert_eq!(slot_idx.bucket_idx, BucketIndex::Bucket00);
    let mut prev = slot_idx;
    for idx in 1..=u32::MAX {
        let slot_idx = SlotIndex::from_index(idx);

        // SAFETY: Ensure indices don't go out of bounds of buckets.
        assert!(slot_idx.index_in_bucket < slot_idx.bucket_idx.capacity());

        if prev.bucket_idx == slot_idx.bucket_idx {
            assert_eq!(prev.index_in_bucket + 1, slot_idx.index_in_bucket);
        } else {
            assert_eq!(slot_idx.index_in_bucket, 0);
        }

        assert_eq!(buckets[slot_idx.bucket_idx], slot_idx.bucket_idx.capacity() as u32);
        assert_eq!(ENTRIES_BY_BUCKET[slot_idx.bucket_idx], slot_idx.bucket_idx.capacity(), "{idx}",);

        prev = slot_idx;
    }
}
