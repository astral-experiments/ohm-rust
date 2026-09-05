use std::borrow::Borrow;
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::ptr;

use super::{IntoPointer, ShardedHashMap, ShardedHashSet, make_hash};

#[test]
fn interning_preserves_colliding_canonical_values() {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Colliding(u32);

    impl Hash for Colliding {
        fn hash<H: Hasher>(&self, state: &mut H) {
            state.write_u8(0);
        }
    }

    crate::sync::set_dyn_thread_safe_mode(true);
    let values: [Colliding; 64] = std::array::from_fn(|i| Colliding(i as u32));
    let interner = ShardedHashMap::<&Colliding, ()>::with_capacity(1);
    let hash = make_hash(&values[0]);
    let initial_capacity = interner.lock_shard_by_hash(hash).capacity();

    for value in &values {
        assert_eq!(make_hash(value), hash);
        let interned = if value.0 % 2 == 0 {
            interner.intern_ref_with_hash(hash, value, || value)
        } else {
            interner.intern_with_hash(hash, *value, |key| {
                assert_eq!(key, *value);
                value
            })
        };
        assert!(ptr::eq(interned, value));
    }
    assert_eq!(interner.len(), values.len());
    assert!(interner.lock_shard_by_hash(hash).capacity() > initial_capacity);

    // Look up equal values at other addresses after the table has grown.
    for value in &values {
        let equal = *value;
        let by_ref = interner.intern_ref_with_hash(hash, &equal, || panic!("existing key"));
        let by_value = interner.intern_with_hash(hash, equal, |_| panic!("existing key"));
        assert!(ptr::eq(by_ref, value));
        assert!(ptr::eq(by_value, value));
    }
}

#[test]
fn cached_hash_interning_preserves_collisions_and_pointer_identity() {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Colliding(u32);

    impl Hash for Colliding {
        fn hash<H: Hasher>(&self, state: &mut H) {
            // Exercise both full-hash collisions and distinct hashes during growth.
            (self.0 / 4).hash(state);
        }
    }

    #[derive(Clone, Copy)]
    struct Key<'a> {
        value: &'a Colliding,
        hash_calls: &'a Cell<usize>,
    }

    impl Borrow<Colliding> for Key<'_> {
        fn borrow(&self) -> &Colliding {
            self.value
        }
    }

    impl PartialEq for Key<'_> {
        fn eq(&self, other: &Self) -> bool {
            self.value == other.value
        }
    }

    impl Eq for Key<'_> {}

    impl Hash for Key<'_> {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.hash_calls.set(self.hash_calls.get() + 1);
            self.value.hash(state);
        }
    }

    impl IntoPointer for Key<'_> {
        fn into_pointer(&self) -> *const () {
            self.value as *const Colliding as *const ()
        }
    }

    crate::sync::set_dyn_thread_safe_mode(true);
    let values: [Colliding; 512] = std::array::from_fn(|i| Colliding(i as u32));
    let hash_calls = Cell::new(0);
    let interner = ShardedHashSet::with_capacity(1);
    let capacity = || interner.lock_shards().map(|shard| shard.capacity()).sum::<usize>();
    let initial_capacity = capacity();

    for value in &values {
        let hash = make_hash(value);
        let key = Key { value, hash_calls: &hash_calls };
        let interned = if value.0 % 2 == 0 {
            interner.intern_ref_with_hash(hash, value, || key)
        } else {
            interner.intern_with_hash(hash, *value, |_| key)
        };
        assert!(ptr::eq(interned.value, value));
    }
    assert_eq!(interner.len(), values.len());
    assert!(capacity() > initial_capacity);
    assert_eq!(hash_calls.get(), 0, "growth must reuse the stored hashes");

    for value in &values {
        let equal = *value;
        let hash = make_hash(&equal);
        let by_ref = interner.intern_ref_with_hash(hash, &equal, || panic!("existing key"));
        let by_value = interner.intern_with_hash(hash, equal, |_| panic!("existing key"));
        assert!(ptr::eq(by_ref.value, value));
        assert!(ptr::eq(by_value.value, value));
        assert!(interner.contains_pointer_to(&by_ref));
        assert!(!interner.contains_pointer_to(&Key { value: &equal, hash_calls: &hash_calls }));
    }
}
