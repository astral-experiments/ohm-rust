use std::cell::Cell;
use std::num::NonZero;
use std::thread;

use rustc_data_structures::sync::Registry;
use rustc_span::def_id::CrateNum;

use super::{DefId, DefIndex, DepNodeIndex, FOREIGN_QUERY_CACHE_LEN, ForeignQueryCache};

fn key(krate: u32, index: u32) -> DefId {
    DefId { krate: CrateNum::from_u32(krate), index: DefIndex::from_u32(index) }
}

fn result(value: u32) -> (u32, DepNodeIndex) {
    (value, DepNodeIndex::from_u32(value + 1))
}

#[test]
fn foreign_query_cache_checks_complete_keys_and_query_scope() {
    thread::spawn(|| {
        Registry::new(NonZero::new(1).unwrap()).register();
        let cache = ForeignQueryCache::default();
        let width = FOREIGN_QUERY_CACHE_LEN as u32;
        let entries = [
            (key(1, 7), result(10)),
            (key(1 + width, 7), result(20)),
            (key(1, 7 + width), result(30)),
        ];
        for (key, expected) in entries.into_iter().cycle().take(9) {
            assert_eq!(cache.get_or_insert_with(key, || Some(expected)), Some(expected));
            assert_eq!(
                cache.get_or_insert_with(key, || panic!("completed result was not cached")),
                Some(expected)
            );
        }
        let other_query = ForeignQueryCache::default();
        assert_eq!(
            other_query.get_or_insert_with(key(1, 7 + width), || Some(result(40))),
            Some(result(40))
        );
        assert_eq!(
            cache.get_or_insert_with(key(1, 7 + width), || panic!("query caches overlap")),
            Some(result(30))
        );
    })
    .join()
    .unwrap();
}

#[test]
fn foreign_query_cache_does_not_cache_incomplete_results() {
    thread::spawn(|| {
        Registry::new(NonZero::new(1).unwrap()).register();
        let cache = ForeignQueryCache::default();
        let key = key(1, 7);
        assert_eq!(cache.get_or_insert_with(key, || None), None);
        assert_eq!(cache.get_or_insert_with(key, || Some(result(10))), Some(result(10)));
        assert_eq!(
            cache.get_or_insert_with(key, || panic!("completed result was not cached")),
            Some(result(10))
        );
    })
    .join()
    .unwrap();
}

#[test]
fn foreign_query_cache_keeps_workers_and_registries_separate() {
    thread::spawn(|| {
        let registry = Registry::new(NonZero::new(2).unwrap());
        registry.register();
        let cache = ForeignQueryCache::default();
        let key = key(1, 7);
        assert_eq!(cache.get_or_insert_with(key, || Some(result(10))), Some(result(10)));
        thread::scope(|scope| {
            scope
                .spawn(|| {
                    registry.register();
                    assert_eq!(
                        cache.get_or_insert_with(key, || Some(result(20))),
                        Some(result(20))
                    );
                    assert_eq!(
                        cache.get_or_insert_with(key, || panic!("worker result was not cached")),
                        Some(result(20))
                    );
                })
                .join()
                .unwrap();
            scope
                .spawn(|| {
                    let calls = Cell::new(0);
                    let check = || {
                        assert_eq!(
                            cache.get_or_insert_with(key, || {
                                calls.set(calls.get() + 1);
                                Some(result(30))
                            }),
                            Some(result(30))
                        );
                    };
                    check();
                    check();
                    Registry::new(NonZero::new(1).unwrap()).register();
                    check();
                    check();
                    assert_eq!(calls.get(), 4);
                })
                .join()
                .unwrap();
        });
        assert_eq!(
            cache.get_or_insert_with(key, || panic!("owner result was replaced")),
            Some(result(10))
        );
    })
    .join()
    .unwrap();
}

#[test]
fn foreign_query_cache_can_be_created_without_a_registry() {
    thread::spawn(|| {
        let cache = ForeignQueryCache::default();
        let key = key(1, 7);
        assert_eq!(cache.get_or_insert_with(key, || Some(result(10))), Some(result(10)));
        assert_eq!(cache.get_or_insert_with(key, || Some(result(20))), Some(result(20)));
        Registry::new(NonZero::new(1).unwrap()).register();
        assert_eq!(cache.get_or_insert_with(key, || Some(result(30))), Some(result(30)));
    })
    .join()
    .unwrap();
}
