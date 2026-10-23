use std::cell::Cell;

use super::{
    CLAUSE_INSTANTIATION_CACHE_LEN, ClauseInstantiationCacheEntry, FN_SIG_INSTANTIATION_CACHE_LEN,
    FnSigInstantiationCacheEntry, LazyWorkerCache, SUPERTRAIT_CACHE_LEN, SupertraitCacheEntry,
};

fn first_use<T: Copy, const N: usize>() {
    let cache = LazyWorkerCache::<T, N>::new();
    assert!(cache.entries.get().is_none());
    assert!(cache[N - 1].get().is_none());
    let entries = cache.entries.get().unwrap();
    assert_eq!(entries.len(), N);
    assert!(entries.iter().all(|entry| entry.get().is_none()));
    assert!(std::ptr::eq(&cache[0], &entries[0]));
    assert!(std::ptr::eq(&cache[N - 1], &entries[N - 1]));
}

#[test]
fn lazy_worker_caches_initialize_each_family_on_first_use() {
    first_use::<SupertraitCacheEntry<'static>, SUPERTRAIT_CACHE_LEN>();
    first_use::<ClauseInstantiationCacheEntry<'static>, CLAUSE_INSTANTIATION_CACHE_LEN>();
    first_use::<FnSigInstantiationCacheEntry<'static>, FN_SIG_INSTANTIATION_CACHE_LEN>();
}

#[test]
fn lazy_worker_cache_preserves_entries_and_colliding_slot_replacement() {
    // Key matching stays in the unchanged consumers. This checks that repeated
    // accesses and replacement of one slot retain the same backing cells.
    let cache = LazyWorkerCache::<(u32, u32, u32), 4>::new();
    cache[1].set(Some((1, 2, 3)));
    cache[2].set(Some((4, 5, 6)));
    let entries = cache.entries.get().unwrap().as_ptr();
    assert_eq!(cache[1].get(), Some((1, 2, 3)));
    assert_eq!(cache[1].get(), Some((1, 2, 3)));
    cache[1].set(Some((7, 8, 9)));
    assert_eq!(cache[1].get(), Some((7, 8, 9)));
    assert_eq!(cache[2].get(), Some((4, 5, 6)));
    assert!(cache[0].get().is_none());
    assert!(cache[3].get().is_none());
    assert_eq!(entries, cache.entries.get().unwrap().as_ptr());
    // Retaining a reference to a slot across other accesses must also be valid.
    let slot: &Cell<_> = &cache[1];
    cache[3].set(Some((10, 11, 12)));
    assert_eq!(slot.get(), Some((7, 8, 9)));
}
