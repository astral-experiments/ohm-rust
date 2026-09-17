use std::cell::UnsafeCell;
use std::sync::OnceLock;

use rustc_data_structures::sharded::{ShardedHashMap, make_hash};
use rustc_data_structures::sync::{DynSend, DynSync};
pub use rustc_data_structures::vec_cache::VecCache;
use rustc_hir::def_id::LOCAL_CRATE;
use rustc_index::Idx;
use rustc_span::def_id::{DefId, DefIndex, LocalDefId};

use crate::dep_graph::DepNodeIndex;
use crate::query::keys::QueryKey;

/// Trait for types that serve as an in-memory cache for query results,
/// for a given key (argument) type and value (return) type.
///
/// Types implementing this trait are associated with actual key/value types
/// by the `Cache` associated type of the `rustc_middle::query::Key` trait.
pub trait QueryCache: Sized {
    type Key: QueryKey;
    type Value: Copy;

    /// Returns the cached value (and other information) associated with the
    /// given key, if it is present in the cache.
    fn lookup(&self, key: &Self::Key) -> Option<(Self::Value, DepNodeIndex)>;

    /// Adds a key/value entry to this cache.
    ///
    /// Called by some part of the query system, after having obtained the
    /// value by executing the query or loading a cached value from disk.
    fn complete(&self, key: Self::Key, value: Self::Value, index: DepNodeIndex);

    /// Calls a closure on each entry in this cache.
    fn for_each(&self, f: &mut dyn FnMut(&Self::Key, &Self::Value, DepNodeIndex));

    /// Returns the number of entries currently in this cache.
    ///
    /// Useful for reserving capacity in data structures that will hold the
    /// output of a call to [`Self::for_each`].
    fn len(&self) -> usize;

    /// Removes exactly one completed query, returning whether the key was cached.
    ///
    /// # Safety
    /// No other cache operation or query may run until eviction completes. All previous cache
    /// accesses must have completed, and the next access must happen after this call.
    unsafe fn invalidate_quiescent(&self, key: Self::Key) -> bool;
}

/// In-memory cache for queries whose keys aren't suitable for any of the
/// more specialized kinds of cache. Backed by a sharded hashmap.
pub struct DefaultCache<K, V> {
    cache: ShardedHashMap<K, (V, DepNodeIndex)>,
}

impl<K, V> Default for DefaultCache<K, V> {
    fn default() -> Self {
        DefaultCache { cache: Default::default() }
    }
}

impl<K, V> QueryCache for DefaultCache<K, V>
where
    K: QueryKey,
    V: Copy,
{
    type Key = K;
    type Value = V;

    #[inline(always)]
    fn lookup(&self, key: &K) -> Option<(V, DepNodeIndex)> {
        self.cache.get(key)
    }

    #[inline]
    fn complete(&self, key: K, value: V, index: DepNodeIndex) {
        self.cache.insert_unique(key, (value, index));
    }

    fn for_each(&self, f: &mut dyn FnMut(&Self::Key, &Self::Value, DepNodeIndex)) {
        for shard in self.cache.lock_shards() {
            for (k, v) in shard.iter() {
                f(k, &v.0, v.1);
            }
        }
    }

    fn len(&self) -> usize {
        self.cache.len()
    }

    unsafe fn invalidate_quiescent(&self, key: K) -> bool {
        let hash = make_hash(&key);
        self.cache
            .lock_shard_by_hash(hash)
            .find_entry(hash, |(stored, _)| *stored == key)
            .map(|entry| entry.remove())
            .is_ok()
    }
}

/// In-memory cache for queries whose key type only has one value (e.g. `()`).
/// The cache therefore only needs to store one query return value.
pub struct SingleCache<V> {
    // The ordinary path still uses OnceLock directly; only a quiescent resident session may
    // obtain exclusive access to reset the cell. No reference to a cached value leaves this type.
    cache: UnsafeCell<OnceLock<(V, DepNodeIndex)>>,
}

// SAFETY: Ordinary access uses OnceLock's synchronization. The only method that mutates the
// OnceLock itself is unsafe and requires all concurrent access to have stopped.
unsafe impl<V: Send + Sync> Sync for SingleCache<V> {}
// SAFETY: The same OnceLock/quiescence invariant applies in rustc's dynamic thread-safety mode.
unsafe impl<V: DynSend + DynSync> DynSync for SingleCache<V> {}

impl<V> Default for SingleCache<V> {
    fn default() -> Self {
        SingleCache { cache: UnsafeCell::new(OnceLock::new()) }
    }
}

impl<V> SingleCache<V> {
    fn cell(&self) -> &OnceLock<(V, DepNodeIndex)> {
        // SAFETY: The only mutable access requires all other accesses to have completed.
        unsafe { &*self.cache.get() }
    }
}

impl<V> QueryCache for SingleCache<V>
where
    V: Copy,
{
    type Key = ();
    type Value = V;

    #[inline(always)]
    fn lookup(&self, _key: &()) -> Option<(V, DepNodeIndex)> {
        self.cell().get().copied()
    }

    #[inline]
    fn complete(&self, _key: (), value: V, index: DepNodeIndex) {
        self.cell().set((value, index)).ok();
    }

    fn for_each(&self, f: &mut dyn FnMut(&Self::Key, &Self::Value, DepNodeIndex)) {
        if let Some(value) = self.cell().get() {
            f(&(), &value.0, value.1)
        }
    }

    fn len(&self) -> usize {
        self.cell().get().is_some().into()
    }

    unsafe fn invalidate_quiescent(&self, _key: ()) -> bool {
        // SAFETY: The caller guarantees that no ordinary cell access is running.
        unsafe { &mut *self.cache.get() }.take().is_some()
    }
}

/// In-memory cache for queries whose key is a [`DefId`].
///
/// Selects between one of two internal caches, depending on whether the key
/// is a local ID or foreign-crate ID.
pub struct DefIdCache<V> {
    /// Stores the local DefIds in a dense map. Local queries are much more often dense, so this is
    /// a win over hashing query keys at marginal memory cost (~5% at most) compared to FxHashMap.
    local: VecCache<DefIndex, V, DepNodeIndex>,
    foreign: DefaultCache<DefId, V>,
}

impl<V> Default for DefIdCache<V> {
    fn default() -> Self {
        DefIdCache { local: Default::default(), foreign: Default::default() }
    }
}

impl<V: Copy> DefIdCache<V> {
    /// Invalidates only a locally defined key in a quiescent resident compiler.
    ///
    /// # Safety
    ///
    /// The caller must provide the same quiescence guarantees as
    /// [`VecCache::ohm_invalidate_quiescent`].
    pub unsafe fn ohm_invalidate_local_quiescent(&self, key: LocalDefId) -> bool {
        // SAFETY: The caller guarantees no other operation can access the cache.
        unsafe { self.local.ohm_invalidate_quiescent(key.local_def_index) }
    }
}

impl<V> QueryCache for DefIdCache<V>
where
    V: Copy,
{
    type Key = DefId;
    type Value = V;

    #[inline(always)]
    fn lookup(&self, key: &DefId) -> Option<(V, DepNodeIndex)> {
        if key.krate == LOCAL_CRATE {
            self.local.lookup(&key.index)
        } else {
            self.foreign.lookup(key)
        }
    }

    #[inline]
    fn complete(&self, key: DefId, value: V, index: DepNodeIndex) {
        if key.krate == LOCAL_CRATE {
            self.local.complete(key.index, value, index)
        } else {
            self.foreign.complete(key, value, index)
        }
    }

    fn for_each(&self, f: &mut dyn FnMut(&Self::Key, &Self::Value, DepNodeIndex)) {
        self.local.for_each(&mut |key, value, index| {
            f(&DefId { krate: LOCAL_CRATE, index: *key }, value, index);
        });
        self.foreign.for_each(f);
    }

    fn len(&self) -> usize {
        self.local.len() + self.foreign.len()
    }

    unsafe fn invalidate_quiescent(&self, key: DefId) -> bool {
        // SAFETY: Both paths inherit the caller's cache-wide quiescence guarantee.
        if key.krate == LOCAL_CRATE {
            unsafe { self.local.ohm_invalidate_quiescent(key.index) }
        } else {
            unsafe { self.foreign.invalidate_quiescent(key) }
        }
    }
}

impl<K, V> QueryCache for VecCache<K, V, DepNodeIndex>
where
    K: Idx + QueryKey,
    V: Copy,
{
    type Key = K;
    type Value = V;

    #[inline(always)]
    fn lookup(&self, key: &K) -> Option<(V, DepNodeIndex)> {
        self.lookup(key)
    }

    #[inline]
    fn complete(&self, key: K, value: V, index: DepNodeIndex) {
        self.complete(key, value, index)
    }

    fn for_each(&self, f: &mut dyn FnMut(&Self::Key, &Self::Value, DepNodeIndex)) {
        self.for_each(f)
    }

    fn len(&self) -> usize {
        self.len()
    }

    unsafe fn invalidate_quiescent(&self, key: K) -> bool {
        // SAFETY: The caller guarantees that no cache operation can overlap eviction.
        unsafe { self.ohm_invalidate_quiescent(key) }
    }
}

#[cfg(test)]
mod tests {
    use rustc_data_structures::sync::set_dyn_thread_safe_mode;
    use rustc_span::def_id::CrateNum;

    use super::*;

    fn def(krate: u32, index: u32) -> DefId {
        DefId { krate: CrateNum::from_u32(krate), index: DefIndex::from_u32(index) }
    }

    #[test]
    fn resident_single_cache_can_be_recompleted() {
        let cache = SingleCache::<u32>::default();
        for value in 0..4 {
            assert!(!unsafe { cache.invalidate_quiescent(()) });
            cache.complete((), value, DepNodeIndex::ZERO);
            assert_eq!(cache.lookup(&()), Some((value, DepNodeIndex::ZERO)));
            assert_eq!(cache.len(), 1);
            assert!(unsafe { cache.invalidate_quiescent(()) });
            assert_eq!(cache.len(), 0);
        }
    }

    #[test]
    fn resident_hash_and_def_id_caches_preserve_other_keys() {
        set_dyn_thread_safe_mode(true);
        let local = def(0, 10);
        let foreign = def(1, 10);
        let absent = def(0, 11);
        let cache = DefIdCache::<u32>::default();
        for (key, value) in [(local, 10), (foreign, 20)] {
            cache.complete(key, value, DepNodeIndex::ZERO);
        }
        assert!(unsafe { cache.invalidate_quiescent(local) });
        assert!(!unsafe { cache.invalidate_quiescent(absent) });
        assert_eq!(cache.lookup(&foreign), Some((20, DepNodeIndex::ZERO)));
        cache.complete(local, 30, DepNodeIndex::ZERO);
        assert!(unsafe { cache.invalidate_quiescent(foreign) });
        assert_eq!(cache.lookup(&local), Some((30, DepNodeIndex::ZERO)));

        let cache = DefaultCache::<(CrateNum, DefId), u32>::default();
        let composite_a = (LOCAL_CRATE, local);
        let composite_b = (foreign.krate, local);
        cache.complete(composite_a, 10, DepNodeIndex::ZERO);
        cache.complete(composite_b, 20, DepNodeIndex::ZERO);
        assert!(unsafe { cache.invalidate_quiescent(composite_a) });
        assert!(!unsafe { cache.invalidate_quiescent(composite_a) });
        assert_eq!(cache.lookup(&composite_b), Some((20, DepNodeIndex::ZERO)));
        cache.complete(composite_a, 30, DepNodeIndex::ZERO);
        assert_eq!(cache.lookup(&composite_a), Some((30, DepNodeIndex::ZERO)));
        assert_eq!(cache.len(), 2);
    }
}
