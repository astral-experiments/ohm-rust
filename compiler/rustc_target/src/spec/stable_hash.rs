use rustc_data_structures::stable_hash::{StableHash, StableHashCtxt, StableHasher};

use super::TargetTuple;

impl StableHash for TargetTuple {
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        match self {
            TargetTuple::TargetTuple(tuple) => {
                0u8.stable_hash(hcx, hasher);
                tuple.stable_hash(hcx, hasher)
            }
            TargetTuple::TargetJson { path_for_rustdoc: _, tuple, contents } => {
                1u8.stable_hash(hcx, hasher);
                tuple.stable_hash(hcx, hasher);
                contents.stable_hash(hcx, hasher)
            }
        }
    }
}
