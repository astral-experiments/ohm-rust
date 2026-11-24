//! This module contains `StableHash` implementations for various data types
//! from `rustc_middle::ty` in no particular order.

use std::cell::RefCell;
use std::ptr;

use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::fx::FxHashMap;
use rustc_data_structures::stable_hash::{
    StableHash, StableHashControls, StableHashCtxt, StableHasher,
};
use tracing::trace;

use crate::{mir, ty};

/// Distinguishes the hashing contexts for interned types and lists containing them.
///
/// Field mutability restrictions carry spans. Metadata hashes their absolute locations,
/// while incremental queries can hash offsets relative to the owning definition.
#[inline]
pub(super) fn type_hash_mode(hcx: &impl StableHashCtxt) -> usize {
    let StableHashControls { hash_spans, incremental_keys } = hcx.stable_hash_controls();
    usize::from(hash_spans)
        + 2 * usize::from(incremental_keys)
        + 4 * usize::from(hcx.hashes_metadata_spans())
}

impl<'tcx, H, T> StableHash for &'tcx ty::list::RawList<H, T>
where
    T: StableHash,
{
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        // Note: this cache makes an *enormous* performance difference on certain benchmarks. E.g.
        // without it, compiling `diesel-2.2.10` can be 74% slower, and compiling
        // `deeply-nested-multi` can be ~4,000x slower(!)
        thread_local! {
            static CACHE: [RefCell<FxHashMap<*const (), Fingerprint>>; 8] = Default::default();
        }

        let hash = CACHE.with(|cache| {
            let cache = &cache[type_hash_mode(hcx)];
            let key = ptr::from_ref(*self).cast::<()>();
            if let Some(&hash) = cache.borrow().get(&key) {
                return hash;
            }

            let mut hasher = StableHasher::new();
            self[..].stable_hash(hcx, &mut hasher);

            let hash: Fingerprint = hasher.finish();
            cache.borrow_mut().insert(key, hash);
            hash
        });

        hash.stable_hash(hcx, hasher);
    }
}

impl<'tcx> StableHash for ty::GenericArg<'tcx> {
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        self.kind().stable_hash(hcx, hasher);
    }
}

// AllocIds get resolved to whatever they point to (to be stable)
impl StableHash for mir::interpret::AllocId {
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        ty::tls::with_opt(|tcx| {
            trace!("hashing {:?}", *self);
            let tcx = tcx.expect("can't hash AllocIds during hir lowering");
            tcx.stable_hash_alloc_id(*self, hcx, hasher);
        });
    }
}

impl StableHash for mir::interpret::CtfeProvenance {
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        self.into_parts().stable_hash(hcx, hasher);
    }
}
