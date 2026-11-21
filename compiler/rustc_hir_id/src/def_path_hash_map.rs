use std::hash::Hash;

use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::owned_slice::OwnedSlice;
use rustc_hashes::Hash64;
use rustc_span::def_id::{CrateNum, DefId, DefIndex, DefPathHash, StableCrateId, StableCrateIdMap};

#[derive(Clone, Default)]
pub struct Config;

impl odht::Config for Config {
    // This hash-map is single-crate, so we only need to key by the local hash.
    type Key = Hash64;
    type Value = DefIndex;

    type EncodedKey = [u8; 8];
    type EncodedValue = [u8; 4];

    type H = odht::UnHashFn;

    #[inline]
    fn encode_key(k: &Hash64) -> [u8; 8] {
        k.as_u64().to_le_bytes()
    }

    #[inline]
    fn encode_value(v: &DefIndex) -> [u8; 4] {
        v.as_u32().to_le_bytes()
    }

    #[inline]
    fn decode_key(k: &[u8; 8]) -> Hash64 {
        Hash64::new(u64::from_le_bytes(*k))
    }

    #[inline]
    fn decode_value(v: &[u8; 4]) -> DefIndex {
        DefIndex::from_u32(u32::from_le_bytes(*v))
    }
}

pub type DefPathHashMap = odht::HashTableOwned<Config>;

pub type MappedDefPathHashMap = odht::HashTable<Config, OwnedSlice>;

/// The producer's alternate definition keys, borrowed directly from its metadata.
pub struct ExternalDefPaths {
    pub id: StableCrateId,
    pub hashes: OwnedSlice,
    pub indices: MappedDefPathHashMap,
}

impl ExternalDefPaths {
    #[inline]
    pub fn hash(&self, index: DefIndex) -> DefPathHash {
        let bytes = self.hashes.as_chunks::<8>().0[index.as_usize()];
        DefPathHash::new(self.id, Hash64::new(u64::from_le_bytes(bytes)))
    }
}

/// The extra keys available for this compilation. Existing canonical lookups keep
/// using the crate store. Insertions happen under its write lock during resolution.
#[derive(Default)]
pub struct ExternalIncrementalKeys {
    by_id: StableCrateIdMap,
    paths: Vec<Option<Box<RegisteredPaths>>>,
}

struct RegisteredPaths {
    paths: ExternalDefPaths,
    canonical: StableCrateId,
    prefix: Fingerprint,
}

impl ExternalIncrementalKeys {
    /// Give every prefix an immutable fingerprint. Stable hashing may read any
    /// canonical ID in the prefix, including through precomputed fingerprints.
    /// A single query on the last crate covers all such reads. Keying it by that
    /// crate lets resolution append dependencies after the query is first forced.
    pub fn insert(&mut self, krate: CrateNum, canonical: StableCrateId, paths: ExternalDefPaths) {
        let mut hasher = rustc_data_structures::stable_hash::StableHasher::new();
        if let Some(last) = self.last() {
            self.prefix(last).hash(&mut hasher);
        }
        paths.id.as_u64().hash(&mut hasher);
        canonical.as_u64().hash(&mut hasher);
        let fingerprint = hasher.finish();
        assert!(self.by_id.insert(paths.id, krate).is_none());
        let index = krate.as_usize();
        if self.paths.len() <= index {
            self.paths.resize_with(index + 1, || None);
        }
        self.paths[index] =
            Some(Box::new(RegisteredPaths { paths, canonical, prefix: fingerprint }));
    }

    #[inline]
    pub fn get(&self, krate: CrateNum) -> Option<&ExternalDefPaths> {
        Some(&self.paths.get(krate.as_usize())?.as_ref()?.paths)
    }

    pub fn canonical(&self, krate: CrateNum) -> StableCrateId {
        self.paths[krate.as_usize()].as_ref().unwrap().canonical
    }

    pub fn prefix(&self, krate: CrateNum) -> Fingerprint {
        self.paths[krate.as_usize()].as_ref().unwrap().prefix
    }

    pub fn last(&self) -> Option<CrateNum> {
        self.by_id.last().map(|(_, &n)| n)
    }

    pub fn crate_num(&self, id: StableCrateId) -> Option<CrateNum> {
        self.by_id.get(&id).copied()
    }

    pub fn def_id(&self, hash: DefPathHash) -> Option<DefId> {
        let krate = self.crate_num(hash.stable_crate_id())?;
        let index = self.get(krate)?.indices.get(&hash.local_hash())?;
        Some(DefId { krate, index })
    }
}
