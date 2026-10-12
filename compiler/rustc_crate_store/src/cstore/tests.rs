use std::num::NonZero;

use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::sync::Registry;
use rustc_span::def_id::DefIndex;

use super::*;

fn def_id(krate: u32, index: u32) -> DefId {
    DefId { krate: CrateNum::from_u32(krate), index: DefIndex::from_u32(index) }
}

fn def_hash(value: u8) -> DefPathHash {
    DefPathHash(Fingerprint::from_le_bytes([value; 16]))
}

#[test]
fn foreign_hash_cache_checks_complete_keys_and_store_scope() {
    std::thread::spawn(|| {
        Registry::new(NonZero::new(1).unwrap()).register();
        let cache = ForeignDefPathHashCache::new();
        let width = FOREIGN_DEF_PATH_HASH_CACHE_LEN as u32;
        // These distinct crate/index combinations collide in the bounded cache.
        let entries = [
            (def_id(1, 7), def_hash(1)),
            (def_id(1 + width, 7), def_hash(2)),
            (def_id(1, 7 + width), def_hash(3)),
        ];
        for (key, expected) in entries.into_iter().cycle().take(9) {
            assert_eq!(cache.get_or_insert_with(key, || expected), expected);
            assert_eq!(
                cache.get_or_insert_with(key, || panic!("cached hash was not reused")),
                expected
            );
        }

        let other_store = ForeignDefPathHashCache::new();
        assert_eq!(other_store.get_or_insert_with(def_id(1, 7), || def_hash(4)), def_hash(4));
        assert_eq!(cache.get_or_insert_with(def_id(1, 7), || def_hash(1)), def_hash(1));
    })
    .join()
    .unwrap();
}

#[test]
fn unregistered_foreign_hash_lookups_use_the_loader() {
    std::thread::spawn(|| {
        Registry::new(NonZero::new(1).unwrap()).register();
        let cache = ForeignDefPathHashCache::new();
        let key = def_id(1, 7);
        let expected = def_hash(1);
        assert_eq!(cache.get_or_insert_with(key, || expected), expected);
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let calls = Cell::new(0);
                    for _ in 0..2 {
                        assert_eq!(
                            cache.get_or_insert_with(key, || {
                                calls.set(calls.get() + 1);
                                expected
                            }),
                            expected
                        );
                    }
                    assert_eq!(calls.get(), 2);
                })
                .join()
                .unwrap();
        });
    })
    .join()
    .unwrap();
}
