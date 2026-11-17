use std::hash::{Hash, Hasher};
use std::ptr;

use super::{ShardedHashMap, make_hash};

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
