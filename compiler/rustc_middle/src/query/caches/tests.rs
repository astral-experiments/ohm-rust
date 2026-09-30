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
