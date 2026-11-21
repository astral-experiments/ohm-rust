use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::{fmt, mem};

use rustc_data_structures::fx::{FxHashMap, FxIndexMap, FxIndexSet};
use rustc_data_structures::memmap::Mmap;
use rustc_data_structures::sync::{HashMapExt, Lock, RwLock};
use rustc_data_structures::unhash::UnhashMap;
use rustc_data_structures::unord::{UnordMap, UnordSet};
use rustc_hir::def_id::{
    CrateNum, DefId, DefIdSet, DefIndex, LOCAL_CRATE, LocalDefId, StableCrateId,
};
use rustc_hir::definitions::DefPathHash;
use rustc_index::IndexVec;
use rustc_index::bit_set::GrowableBitSet;
use rustc_macros::{Decodable, Encodable};
use rustc_serialize::opaque::{
    FileEncodeResult, FileEncoder, IntEncodedWithFixedSize, MAGIC_END_BYTES, MemDecoder,
};
use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};
use rustc_session::Session;
use rustc_span::hygiene::{
    ExpnId, HygieneDecodeContext, HygieneEncodeContext, SyntaxContext, SyntaxContextKey,
};
use rustc_span::{
    BlobDecoder, BytePos, ByteSymbol, CachingSourceMapView, ExpnData, ExpnHash, RelativeBytePos,
    SourceFile, Span, SpanDecoder, SpanEncoder, Spanned, StableSourceFileId, Symbol, bug,
};

use crate::dep_graph::{DepKind, DepNodeIndex, QuerySideEffect, SerializedDepNodeIndex};
use crate::mir::interpret::{AllocDecodingSession, AllocDecodingState};
use crate::mir::{self, interpret};
use crate::mono::{CodegenUnit, MonoItem};
use crate::query::{QueryCache, erase};
use crate::ty::codec::{RefDecodable, TyDecoder, TyEncoder, forward_all_decoder_methods_to};
use crate::ty::{self, Ty, TyCtxt};

const TAG_FILE_FOOTER: u128 = 0xC0FFEE_C0FFEE_C0FFEE_C0FFEE_C0FFEE;
const TAG_SYNTAX_FOOTER: u128 = TAG_FILE_FOOTER + 1;
const MAX_CACHE_GENERATIONS: usize = 3;
static NEXT_CACHE_ID: AtomicU32 = AtomicU32::new(1);

// A normal span encoded with both location information and a `SyntaxContext`
const TAG_FULL_SPAN: u8 = 0;
// A partial span with no location information, encoded only with a `SyntaxContext`
const TAG_PARTIAL_SPAN: u8 = 1;
const TAG_RELATIVE_SPAN: u8 = 2;

const TAG_SYNTAX_CONTEXT: u8 = 0;
const TAG_EXPN_DATA: u8 = 1;

// Tags for encoding Symbols and ByteSymbols
const SYMBOL_STR: u8 = 0;
const SYMBOL_OFFSET: u8 = 1;
const SYMBOL_PREDEFINED: u8 = 2;

/// Provides an interface to incremental compilation data cached from the
/// previous compilation session. This data will eventually include the results
/// of a few selected queries (like `typeck` and `mir_optimized`) and
/// any side effects that have been emitted during a query.
///
/// Green values can remain in their original serialized form. Each retained
/// generation is a prefix of the file with its own footer and decoding context;
/// aliases map current dependency-node indices to indices in the previous generation.
/// Keeping absolute byte positions preserves type and symbol shorthands. If the
/// most recent generation is small and kept the same indices, we can replace it
/// instead of appending another. Otherwise, at `MAX_CACHE_GENERATIONS` normal
/// promotion re-encodes live values into a fresh file.
pub struct OnDiskCache {
    /// Type shorthands are offsets in one cache lineage, not process-global keys.
    cache_id: u32,
    // The complete cache data in serialized form.
    serialized_data: RwLock<Option<Arc<Mmap>>>,
    start_pos: usize,
    data_len: usize,
    /// End of the last complete generation, before any parsed module snapshots.
    retained_len: usize,
    generation_count: usize,
    /// Reuse the previous generation and re-encode only this generation's values.
    replace_overlay: bool,
    previous_cache: Option<Box<OnDiskCache>>,
    /// Presence of either a saved value or an alias to the preceding generation.
    saved_query_slots: GrowableBitSet<SerializedDepNodeIndex>,
    /// Most retained rows keep their graph index. Only remapped rows need a second index.
    remapped_queries: FxHashMap<SerializedDepNodeIndex, SerializedDepNodeIndex>,
    loaded_query_values: Lock<GrowableBitSet<SerializedDepNodeIndex>>,
    pending_query_values: Lock<ForwardedQueries>,

    file_index_to_stable_id: FxHashMap<SourceFileIndex, EncodedSourceFileId>,

    // Caches that are populated lazily during decoding.
    file_index_to_file: Lock<FxHashMap<SourceFileIndex, Arc<SourceFile>>>,

    /// For query dep nodes that have a disk-cached return value, maps the node
    /// index to the position of its serialized value in `serialized_data`.
    query_values_index: FxHashMap<SerializedDepNodeIndex, AbsoluteBytePos>,

    /// For `DepKind::SideEffect` dep nodes, maps the node index to the position
    /// of its serialized [`QuerySideEffect`] in `serialized_data`.
    side_effects_index: FxHashMap<SerializedDepNodeIndex, AbsoluteBytePos>,

    alloc_decoding_state: Arc<AllocDecodingState>,

    // A map from syntax context ids to the position of their associated
    // `SyntaxContextData`. We use a `u32` instead of a `SyntaxContext`
    // to represent the fact that we are storing *encoded* ids. When we decode
    // a `SyntaxContext`, a new id will be allocated from the global `HygieneData`,
    // which will almost certainly be different than the serialized id.
    syntax_contexts: FxHashMap<u32, AbsoluteBytePos>,
    // A map from the `DefPathHash` of an `ExpnId` to the position
    // of their associated `ExpnData`. Ideally, we would store a `DefId`,
    // but we need to decode this before we've constructed a `TyCtxt` (which
    // makes it difficult to decode a `DefId`).

    // Note that these `DefPathHashes` correspond to both local and foreign
    // `ExpnData` (e.g `ExpnData.krate` may not be `LOCAL_CRATE`). Alternatively,
    // we could look up the `ExpnData` from the metadata of foreign crates,
    // but it seemed easier to have `OnDiskCache` be independent of the `CStore`.
    expn_data: UnhashMap<ExpnHash, AbsoluteBytePos>,
    // Additional information used when decoding hygiene data.
    hygiene_context: HygieneDecodeContext,
    // Maps `ExpnHash`es to their raw value from the *previous*
    // compilation session. This is used as an initial 'guess' when
    // we try to map an `ExpnHash` to its value in the current
    // compilation session.
    foreign_expn_data: UnhashMap<ExpnHash, u32>,
}

/// With retained graph indices, a bitmap describes all aliases. Graph
/// compaction can move indices, which need an explicit pair.
#[derive(Default, Encodable, Decodable)]
struct ForwardedQueries {
    unchanged: GrowableBitSet<SerializedDepNodeIndex>,
    moved: Vec<(SerializedDepNodeIndex, SerializedDepNodeIndex)>,
}

impl ForwardedQueries {
    fn is_empty(&self) -> bool {
        self.moved.is_empty() && self.unchanged.is_empty()
    }
}

// This type is used only for serialization and deserialization.
#[derive(Encodable, Decodable)]
struct Footer {
    previous_cache_len: Option<usize>,
    forwarded_query_values: ForwardedQueries,
    forwarded_allocations: Vec<(usize, usize, usize)>,
    file_index_to_stable_id: FxHashMap<SourceFileIndex, EncodedSourceFileId>,
    query_values_index: Vec<(SerializedDepNodeIndex, AbsoluteBytePos)>,
    side_effects_index: Vec<(SerializedDepNodeIndex, AbsoluteBytePos)>,
    // The location of all allocations.
    // Most uses only need values up to u32::MAX, but benchmarking indicates that we can use a u64
    // without measurable overhead. This permits larger const allocations without ICEing.
    interpret_alloc_index: Vec<u64>,
    // See `OnDiskCache.syntax_contexts`
    syntax_contexts: FxHashMap<u32, AbsoluteBytePos>,
    // See `OnDiskCache.expn_data`
    expn_data: UnhashMap<ExpnHash, AbsoluteBytePos>,
    foreign_expn_data: UnhashMap<ExpnHash, u32>,
}

/// Serialized values retained in their original decoding context.
pub struct QueryCachePromotion {
    data: Arc<Mmap>,
    data_len: usize,
    start_pos: usize,
    query_values: ForwardedQueries,
    allocation_origins: FxHashMap<interpret::AllocId, (usize, usize)>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Encodable, Decodable)]
struct SourceFileIndex(u32);

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Encodable, Decodable)]
struct AbsoluteBytePos(u64);

impl AbsoluteBytePos {
    #[inline]
    fn new(pos: usize) -> AbsoluteBytePos {
        AbsoluteBytePos(pos.try_into().expect("Incremental cache file size overflowed u64."))
    }

    #[inline]
    fn to_usize(self) -> usize {
        self.0 as usize
    }
}

#[derive(Encodable, Decodable, Clone, Debug)]
struct EncodedSourceFileId {
    stable_source_file_id: StableSourceFileId,
    stable_crate_id: StableCrateId,
}

impl EncodedSourceFileId {
    #[inline]
    fn new(tcx: TyCtxt<'_>, file: &SourceFile) -> EncodedSourceFileId {
        EncodedSourceFileId {
            stable_source_file_id: file.incremental_id,
            stable_crate_id: tcx.incremental_crate_id(file.cnum),
        }
    }
}

impl OnDiskCache {
    /// Creates a new `OnDiskCache` instance from the serialized data in `data`.
    ///
    /// The serialized cache has some basic integrity checks, if those checks indicate that the
    /// on-disk data is corrupt, an error is returned.
    pub fn new(sess: &Session, data: Mmap, start_pos: usize) -> Result<Self, ()> {
        assert!(sess.opts.incremental.is_some());

        let data_len = data.len();
        let cache_id = NEXT_CACHE_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("incremental cache namespace exhausted");
        Self::new_shared(Arc::new(data), start_pos, data_len, 0, cache_id)
    }

    /// Reads one generation while sharing its backing storage with retained generations.
    fn new_shared(
        data: Arc<Mmap>,
        start_pos: usize,
        data_len: usize,
        depth: usize,
        cache_id: u32,
    ) -> Result<Self, ()> {
        if depth >= MAX_CACHE_GENERATIONS {
            return Err(());
        }

        let mut decoder = MemDecoder::new(&data[..data_len], start_pos)?;

        fn footer_position(decoder: &mut MemDecoder<'_>) -> Result<usize, ()> {
            let end = decoder.len().checked_sub(IntEncodedWithFixedSize::ENCODED_SIZE).ok_or(())?;
            let pos = decoder.with_position(end, |d| IntEncodedWithFixedSize::decode(d).0);
            (pos < end as u64).then_some(pos as usize).ok_or(())
        }

        let mut footer_pos = footer_position(&mut decoder)?;
        let mut retained_len = data_len;
        let mut syntax: Vec<(SerializedDepNodeIndex, AbsoluteBytePos)> = Vec::new();
        if depth == 0 && decoder.with_position(footer_pos, |d| d.read_u128()) == TAG_SYNTAX_FOOTER {
            (retained_len, syntax) =
                decoder.with_position(footer_pos, |d| decode_tagged(d, TAG_SYNTAX_FOOTER));
            if retained_len <= start_pos
                || retained_len > footer_pos
                || syntax
                    .iter()
                    .any(|(_, pos)| !(retained_len..footer_pos).contains(&pos.to_usize()))
            {
                return Err(());
            }
            decoder = MemDecoder::new(&data[..retained_len], start_pos)?;
            footer_pos = footer_position(&mut decoder)?;
        }
        // Decode the file footer, which contains all the lookup tables, etc.
        let footer: Footer =
            decoder.with_position(footer_pos, |decoder| decode_tagged(decoder, TAG_FILE_FOOTER));

        let previous_cache = if let Some(previous_len) = footer.previous_cache_len {
            if previous_len <= start_pos || previous_len > footer_pos {
                return Err(());
            }
            Some(Box::new(Self::new_shared(
                Arc::clone(&data),
                start_pos,
                previous_len,
                depth + 1,
                cache_id,
            )?))
        } else {
            if !footer.forwarded_query_values.is_empty() {
                return Err(());
            }
            None
        };
        let generation_count =
            previous_cache.as_ref().map_or(1, |cache| cache.generation_count + 1);
        let mut aliases = if footer.forwarded_allocations.is_empty() {
            Vec::new()
        } else {
            vec![None; footer.interpret_alloc_index.len()]
        };
        for (index, depth, previous_index) in footer.forwarded_allocations {
            if depth == 0
                || depth >= generation_count
                || index >= aliases.len()
                || aliases[index].is_some()
            {
                return Err(());
            }
            let mut ancestor = previous_cache.as_deref();
            for _ in 1..depth {
                ancestor = ancestor.and_then(|cache| cache.previous_cache.as_deref());
            }
            let Some(ancestor) = ancestor else { return Err(()) };
            if previous_index >= ancestor.alloc_decoding_state.len() {
                return Err(());
            }
            aliases[index] = Some((Arc::clone(&ancestor.alloc_decoding_state), previous_index));
        }
        let alloc_decoding_state =
            Arc::new(AllocDecodingState::new(footer.interpret_alloc_index).with_aliases(aliases));

        let ForwardedQueries { unchanged: mut saved_query_slots, moved } =
            footer.forwarded_query_values;
        // Bound the number of queries to promote and the bytes to re-encode.
        // Let normal compaction discard the prefix if few of its queries remain.
        let replace_overlay = moved.is_empty()
            && previous_cache.as_ref().is_some_and(|prev| {
                let saved = prev.saved_query_slots.count();
                footer_pos - prev.data_len <= prev.data_len / 8
                    && footer.query_values_index.len() <= saved / 8
                    && saved_query_slots.count() >= saved / 2
            });
        let mut remapped_queries = FxHashMap::default();
        for (index, previous) in moved {
            saved_query_slots.insert(index);
            remapped_queries.insert(index, previous);
        }
        for &(index, _) in &footer.query_values_index {
            saved_query_slots.insert(index);
        }

        for &(index, _) in &syntax {
            if !saved_query_slots.insert(index) {
                return Err(());
            }
        }

        Ok(Self {
            cache_id,
            serialized_data: RwLock::new(Some(data)),
            start_pos,
            data_len,
            retained_len,
            generation_count,
            replace_overlay,
            previous_cache,
            saved_query_slots,
            remapped_queries,
            loaded_query_values: Default::default(),
            pending_query_values: Default::default(),
            file_index_to_stable_id: footer.file_index_to_stable_id,
            file_index_to_file: Default::default(),
            query_values_index: footer.query_values_index.into_iter().chain(syntax).collect(),
            side_effects_index: footer.side_effects_index.into_iter().collect(),
            alloc_decoding_state,
            syntax_contexts: footer.syntax_contexts,
            expn_data: footer.expn_data,
            foreign_expn_data: footer.foreign_expn_data,
            hygiene_context: Default::default(),
        })
    }

    pub fn new_empty() -> Self {
        Self {
            cache_id: 0,
            serialized_data: RwLock::new(None),
            start_pos: 0,
            data_len: 0,
            retained_len: 0,
            generation_count: 0,
            replace_overlay: false,
            previous_cache: None,
            saved_query_slots: Default::default(),
            remapped_queries: Default::default(),
            loaded_query_values: Default::default(),
            pending_query_values: Default::default(),
            file_index_to_stable_id: Default::default(),
            file_index_to_file: Default::default(),
            query_values_index: Default::default(),
            side_effects_index: Default::default(),
            alloc_decoding_state: Arc::new(AllocDecodingState::new(Vec::new())),
            syntax_contexts: FxHashMap::default(),
            expn_data: UnhashMap::default(),
            foreign_expn_data: UnhashMap::default(),
            hygiene_context: Default::default(),
        }
    }

    /// Release the serialized backing `Mmap`.
    pub fn close_serialized_data_mmap(&self) {
        // Obtain a write lock, and replace the mmap with None to drop it.
        *self.serialized_data.write() = None;
        if let Some(previous_cache) = &self.previous_cache {
            previous_cache.close_serialized_data_mmap();
        }
    }

    /// Retains an eligible, green query in its original serialized form.
    pub fn try_forward_query_value(
        &self,
        kind: DepKind,
        previous_index: SerializedDepNodeIndex,
        current_index: DepNodeIndex,
    ) -> bool {
        // Module syntax lives outside the retained prefix.
        if kind == DepKind::ohm_module_syntax || !self.loadable_from_disk(previous_index) {
            return false;
        }
        if self.replace_overlay {
            // All aliases point at the same slot in the previous generation. An
            // explicitly encoded value shadows its alias and must be re-encoded.
            if self.query_values_index.contains_key(&previous_index) {
                return false;
            }
        } else if self.generation_count >= MAX_CACHE_GENERATIONS {
            return false;
        }
        let current_index = SerializedDepNodeIndex::from_curr_for_serialization(current_index);
        let mut pending = self.pending_query_values.lock();
        if current_index == previous_index {
            pending.unchanged.insert(current_index);
        } else {
            pending.moved.push((current_index, previous_index));
        }
        true
    }

    /// Retains serialized data while replacing the cache file.
    pub fn take_query_cache_promotion(&self) -> Option<QueryCachePromotion> {
        let mut query_values = mem::take(&mut *self.pending_query_values.lock());
        if query_values.is_empty() {
            return None;
        }
        query_values.moved.sort_unstable_by_key(|(index, _)| index.as_u32());
        let mut allocation_origins = FxHashMap::default();
        let retained =
            if self.replace_overlay { self.previous_cache.as_deref().unwrap() } else { self };
        let mut cache = Some(retained);
        let mut depth = 1;
        while let Some(current) = cache {
            for (index, id) in current.alloc_decoding_state.decoded_alloc_ids() {
                allocation_origins.entry(id).or_insert((depth, index));
            }
            cache = current.previous_cache.as_deref();
            depth += 1;
        }
        let data = retained.serialized_data.read();
        // The replacement is written separately, so the old mappings remain valid
        // until serialization finishes, including queries loaded while encoding.
        let data = Arc::clone(data.as_ref().unwrap());
        Some(QueryCachePromotion {
            data,
            data_len: retained.retained_len,
            start_pos: retained.start_pos,
            query_values,
            allocation_origins,
        })
    }

    /// Serialize the current-session data that will be loaded by [`OnDiskCache`]
    /// in a subsequent incremental compilation session.
    pub fn serialize(
        tcx: TyCtxt<'_>,
        mut encoder: FileEncoder<'static>,
        promotion: Option<QueryCachePromotion>,
    ) -> FileEncodeResult {
        // Serializing the `DepGraph` should not modify it.
        tcx.dep_graph.with_ignore(|| {
            let (previous_cache_len, forwarded_query_values, allocation_origins) =
                if let Some(promotion) = promotion {
                    // Preserve every old byte position, including type and symbol shorthands.
                    // The already-written header belongs to the same compiler version.
                    assert_eq!(encoder.position(), promotion.start_pos);
                    encoder
                        .emit_raw_bytes(&promotion.data[promotion.start_pos..promotion.data_len]);
                    (Some(promotion.data_len), promotion.query_values, promotion.allocation_origins)
                } else {
                    (None, ForwardedQueries::default(), FxHashMap::default())
                };

            // Allocate `SourceFileIndex`es.
            let (file_to_file_index, file_index_to_stable_id) = {
                let files = tcx.sess.source_map().files();
                let mut file_to_file_index =
                    FxHashMap::with_capacity_and_hasher(files.len(), Default::default());
                let mut file_index_to_stable_id =
                    FxHashMap::with_capacity_and_hasher(files.len(), Default::default());

                for (index, file) in files.iter().enumerate() {
                    let index = SourceFileIndex(index as u32);
                    let file_ptr: *const SourceFile = &raw const **file;
                    file_to_file_index.insert(file_ptr, index);
                    let source_file_id = EncodedSourceFileId::new(tcx, file);
                    file_index_to_stable_id.insert(index, source_file_id);
                }

                (file_to_file_index, file_index_to_stable_id)
            };

            let forwarded_queries = if let Some(cache) = &tcx.query_system.on_disk_cache {
                let loaded = cache.loaded_query_values.lock();
                // A recomputed query can be green even when its diagnostic spans changed.
                // Only skip encoding a live value if we actually decoded its previous bytes.
                let mut forwarded_queries = loaded.intersection(&forwarded_query_values.unchanged);
                for &(index, previous) in &forwarded_query_values.moved {
                    if loaded.contains(previous) {
                        forwarded_queries.insert(index);
                    }
                }
                forwarded_queries
            } else {
                GrowableBitSet::default()
            };

            let mut encoder = CacheEncoder {
                tcx,
                encoder,
                type_shorthands: Default::default(),
                predicate_shorthands: Default::default(),
                interpret_allocs: Default::default(),
                caching_source_map_view: CachingSourceMapView::new(tcx.sess.source_map()),
                file_to_file_index,
                hygiene_context: Default::default(),
                symbol_index_table: Default::default(),
                query_values_index: Default::default(),
                forwarded_queries,
                side_effects_index: Default::default(),
            };

            // Encode query return values.
            tcx.sess.time("encode_query_values", || {
                tcx.encode_query_values(&mut encoder);
            });

            // Encode side effects.
            for (&dep_node_index, side_effect) in tcx.query_system.side_effects.borrow().iter() {
                encoder.encode_side_effect(dep_node_index, side_effect);
            }

            let mut forwarded_allocations = Vec::new();
            let interpret_alloc_index = {
                let mut interpret_alloc_index = Vec::new();
                let mut n = 0;
                loop {
                    let new_n = encoder.interpret_allocs.len();
                    // If we have found new IDs, serialize those too.
                    if n == new_n {
                        // Otherwise, abort.
                        break;
                    }
                    interpret_alloc_index.reserve(new_n - n);
                    for idx in n..new_n {
                        let id = encoder.interpret_allocs[idx];
                        if let Some(&(depth, previous_index)) = allocation_origins.get(&id) {
                            forwarded_allocations.push((idx, depth, previous_index));
                        }
                        let pos: u64 = encoder.position().try_into().unwrap();
                        interpret_alloc_index.push(pos);
                        interpret::specialized_encode_alloc_id(&mut encoder, tcx, id);
                    }
                    n = new_n;
                }
                interpret_alloc_index
            };

            let mut syntax_contexts = FxHashMap::default();
            let mut expn_data = UnhashMap::default();
            let mut foreign_expn_data = UnhashMap::default();

            // Encode all hygiene data (`SyntaxContextData` and `ExpnData`) from the current
            // session.

            HygieneEncodeContext::encode(
                &Rc::clone(&encoder.hygiene_context),
                &mut encoder,
                |encoder, index, _ctxt, ctxt_data| {
                    let pos = AbsoluteBytePos::new(encoder.position());
                    encoder.encode_tagged(TAG_SYNTAX_CONTEXT, ctxt_data);
                    syntax_contexts.insert(index, pos);
                },
                |encoder, expn_id, data, hash| {
                    if expn_id.krate == LOCAL_CRATE {
                        let pos = AbsoluteBytePos::new(encoder.position());
                        encoder.encode_tagged(TAG_EXPN_DATA, data.expect("local expn"));
                        expn_data.insert(expn_id.incremental_expn_hash(), pos);
                    } else {
                        foreign_expn_data.insert(hash, expn_id.local_id.as_u32());
                    }
                },
            );

            // Encode the file footer.
            let footer_pos = encoder.position() as u64;
            let query_values_index = mem::take(&mut encoder.query_values_index);
            let side_effects_index = mem::take(&mut encoder.side_effects_index);
            encoder.encode_tagged(
                TAG_FILE_FOOTER,
                &Footer {
                    previous_cache_len,
                    forwarded_query_values,
                    forwarded_allocations,
                    file_index_to_stable_id,
                    query_values_index,
                    side_effects_index,
                    interpret_alloc_index,
                    syntax_contexts,
                    expn_data,
                    foreign_expn_data,
                },
            );

            IntEncodedWithFixedSize(footer_pos).encode(&mut encoder.encoder);

            // Expansion already reads the parsed modules. Write their opaque
            // byte slices once, after the last complete generation. Editing a
            // large module won't copy its stale syntax into the next file or
            // trigger promotion of unrelated query results.
            let syntax = &tcx.query_system.query_vtables.ohm_module_syntax;
            if syntax.cache.len() != 0 {
                encoder.encoder.emit_raw_bytes(MAGIC_END_BYTES);
                let retained_len = encoder.position();
                syntax.cache.for_each(&mut |key, value, index| {
                    if syntax.will_cache_on_disk_for_key(*key) {
                        encoder.encode_query_value(index, &erase::restore_val::<&[u8]>(*value));
                    }
                });
                let index = mem::take(&mut encoder.query_values_index);
                let footer_pos = encoder.position() as u64;
                encoder.encode_tagged(TAG_SYNTAX_FOOTER, &(retained_len, index));
                IntEncodedWithFixedSize(footer_pos).encode(&mut encoder.encoder);
            }
            encoder.finish()
        })
    }

    /// Loads a `QuerySideEffect` created during the previous compilation session.
    pub(crate) fn load_side_effect(
        &self,
        tcx: TyCtxt<'_>,
        dep_node_index: SerializedDepNodeIndex,
    ) -> Option<QuerySideEffect> {
        let side_effect: Option<QuerySideEffect> =
            self.load_indexed(tcx, dep_node_index, &self.side_effects_index);
        side_effect
    }

    /// Returns true if there is a disk-cached query return value for the given node.
    #[inline]
    pub(crate) fn loadable_from_disk(&self, dep_node_index: SerializedDepNodeIndex) -> bool {
        self.saved_query_slots.contains(dep_node_index)
    }

    /// Dependency nodes with saved query values, in previous-graph order.
    pub(crate) fn cached_query_indices(
        &self,
        slots: usize,
    ) -> impl Iterator<Item = SerializedDepNodeIndex> {
        self.saved_query_slots.iter().take_while(move |index| index.as_usize() < slots)
    }

    /// Returns the disk-cached query return value for the given node, if there is one.
    pub fn try_load_query_value<'tcx, T>(
        &self,
        tcx: TyCtxt<'tcx>,
        dep_node_index: SerializedDepNodeIndex,
    ) -> Option<T>
    where
        T: for<'a> Decodable<CacheDecoder<'a, 'tcx>>,
    {
        let mut cache = self;
        let mut index = dep_node_index;
        loop {
            if let Some(value) = cache.load_indexed(tcx, index, &cache.query_values_index) {
                self.loaded_query_values.lock().insert(dep_node_index);
                return Some(value);
            }
            if !cache.loadable_from_disk(index) {
                return None;
            }
            index = cache.remapped_queries.get(&index).copied().unwrap_or(index);
            cache = cache.previous_cache.as_deref()?;
        }
    }

    fn load_indexed<'tcx, T>(
        &self,
        tcx: TyCtxt<'tcx>,
        dep_node_index: SerializedDepNodeIndex,
        index: &FxHashMap<SerializedDepNodeIndex, AbsoluteBytePos>,
    ) -> Option<T>
    where
        T: for<'a> Decodable<CacheDecoder<'a, 'tcx>>,
    {
        let pos = index.get(&dep_node_index).cloned()?;
        let value = self.with_decoder(tcx, pos, |decoder| decode_tagged(decoder, dep_node_index));
        Some(value)
    }

    fn with_decoder<'a, 'tcx, T, F: for<'s> FnOnce(&mut CacheDecoder<'s, 'tcx>) -> T>(
        &self,
        tcx: TyCtxt<'tcx>,
        pos: AbsoluteBytePos,
        f: F,
    ) -> T
    where
        T: Decodable<CacheDecoder<'a, 'tcx>>,
    {
        let serialized_data = self.serialized_data.read();
        let mut decoder = CacheDecoder {
            cache_id: self.cache_id,
            tcx,
            opaque: MemDecoder::new(
                serialized_data.as_ref().map_or(&[], |data| &data[..self.data_len]),
                pos.to_usize(),
            )
            .unwrap(),
            file_index_to_file: &self.file_index_to_file,
            file_index_to_stable_id: &self.file_index_to_stable_id,
            alloc_decoding_session: self.alloc_decoding_state.new_decoding_session(),
            syntax_contexts: &self.syntax_contexts,
            expn_data: &self.expn_data,
            foreign_expn_data: &self.foreign_expn_data,
            hygiene_context: &self.hygiene_context,
        };
        f(&mut decoder)
    }
}

//- DECODING -------------------------------------------------------------------

/// A decoder that can read from the incremental compilation cache. It is similar to the one
/// we use for crate metadata decoding in that it can rebase spans and eventually
/// will also handle things that contain `Ty` instances.
pub struct CacheDecoder<'a, 'tcx> {
    cache_id: u32,
    tcx: TyCtxt<'tcx>,
    opaque: MemDecoder<'a>,
    file_index_to_file: &'a Lock<FxHashMap<SourceFileIndex, Arc<SourceFile>>>,
    file_index_to_stable_id: &'a FxHashMap<SourceFileIndex, EncodedSourceFileId>,
    alloc_decoding_session: AllocDecodingSession<'a>,
    syntax_contexts: &'a FxHashMap<u32, AbsoluteBytePos>,
    expn_data: &'a UnhashMap<ExpnHash, AbsoluteBytePos>,
    foreign_expn_data: &'a UnhashMap<ExpnHash, u32>,
    hygiene_context: &'a HygieneDecodeContext,
}

impl<'a, 'tcx> CacheDecoder<'a, 'tcx> {
    #[inline]
    fn file_index_to_file(&self, index: SourceFileIndex) -> Arc<SourceFile> {
        let CacheDecoder { tcx, file_index_to_file, file_index_to_stable_id, .. } = *self;

        Arc::clone(file_index_to_file.borrow_mut().entry(index).or_insert_with(|| {
            let source_file_id = &file_index_to_stable_id[&index];
            let source_file_cnum =
                tcx.crate_num_from_incremental_id(source_file_id.stable_crate_id);

            // If this `SourceFile` is from a foreign crate, then make sure
            // that we've imported all of the source files from that crate.
            // This has usually already been done during macro invocation.
            // However, when encoding query results like `TypeckResults`,
            // we might encode an `AdtDef` for a foreign type (because it
            // was referenced in the body of the function). There is no guarantee
            // that we will load the source files from that crate during macro
            // expansion, so we use `import_source_files` to ensure that the foreign
            // source files are actually imported before we call `source_file_by_stable_id`.
            if source_file_cnum != LOCAL_CRATE {
                self.tcx.import_source_files(source_file_cnum);
            }

            tcx.sess
                .source_map()
                .source_file_by_stable_id(source_file_id.stable_source_file_id)
                .expect("failed to lookup `SourceFile` in new context")
        }))
    }

    // copy&paste impl from rustc_metadata
    #[inline]
    fn decode_symbol_or_byte_symbol<S>(
        &mut self,
        new_from_index: impl Fn(u32) -> S,
        read_and_intern_str_or_byte_str_this: impl Fn(&mut Self) -> S,
        read_and_intern_str_or_byte_str_opaque: impl Fn(&mut MemDecoder<'a>) -> S,
    ) -> S {
        let tag = self.read_u8();

        match tag {
            SYMBOL_STR => read_and_intern_str_or_byte_str_this(self),
            SYMBOL_OFFSET => {
                // read str offset
                let pos = self.read_usize();

                // move to str offset and read
                self.opaque.with_position(pos, |d| read_and_intern_str_or_byte_str_opaque(d))
            }
            SYMBOL_PREDEFINED => new_from_index(self.read_u32()),
            _ => unreachable!(),
        }
    }
}

// Decodes something that was encoded with `encode_tagged()` and verify that the
// tag matches and the correct amount of bytes was read.
fn decode_tagged<D, T, V>(decoder: &mut D, expected_tag: T) -> V
where
    T: Decodable<D> + Eq + fmt::Debug,
    V: Decodable<D>,
    D: Decoder,
{
    let start_pos = decoder.position();

    let actual_tag = T::decode(decoder);
    assert_eq!(actual_tag, expected_tag);
    let value = V::decode(decoder);
    let end_pos = decoder.position();

    let expected_len: u64 = Decodable::decode(decoder);
    assert_eq!((end_pos - start_pos) as u64, expected_len);

    value
}

impl<'a, 'tcx> TyDecoder<'tcx> for CacheDecoder<'a, 'tcx> {
    const CLEAR_CROSS_CRATE: bool = false;

    fn cached_ty_for_shorthand<F>(&mut self, shorthand: usize, or_insert_with: F) -> Ty<'tcx>
    where
        F: FnOnce(&mut Self) -> Ty<'tcx>,
    {
        let tcx = self.tcx;

        let cache_key = ty::CReaderCacheKey { cnum: None, cache_id: self.cache_id, pos: shorthand };

        if let Some(&ty) = tcx.caches.ty_rcache.borrow().get(&cache_key) {
            return ty;
        }

        let ty = or_insert_with(self);
        // This may overwrite the entry, but it should overwrite with the same value.
        tcx.caches.ty_rcache.borrow_mut().insert_same(cache_key, ty);
        ty
    }

    fn with_position<F, R>(&mut self, pos: usize, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        debug_assert!(pos < self.opaque.len());

        let new_opaque = self.opaque.split_at(pos);
        let old_opaque = mem::replace(&mut self.opaque, new_opaque);
        let r = f(self);
        self.opaque = old_opaque;
        r
    }

    fn decode_alloc_id(&mut self) -> interpret::AllocId {
        let alloc_decoding_session = self.alloc_decoding_session;
        alloc_decoding_session.decode_alloc_id(self)
    }
}

impl<'a, 'tcx> rustc_type_ir::InternerDecoder for CacheDecoder<'a, 'tcx> {
    type Interner = TyCtxt<'tcx>;

    #[inline]
    fn interner(&self) -> Self::Interner {
        self.tcx
    }
}

impl<'a, 'tcx> Decoder for CacheDecoder<'a, 'tcx> {
    forward_all_decoder_methods_to!(|self| self.opaque);
}

// This ensures that the `Decodable<opaque::Decoder>::decode` specialization for `Vec<u8>` is used
// when a `CacheDecoder` is passed to `Decodable::decode`. Unfortunately, we have to manually opt
// into specializations this way, given how `CacheDecoder` and the decoding traits currently work.
impl<'a, 'tcx> Decodable<CacheDecoder<'a, 'tcx>> for Vec<u8> {
    fn decode(d: &mut CacheDecoder<'a, 'tcx>) -> Self {
        Decodable::decode(&mut d.opaque)
    }
}

impl<'a, 'tcx> SpanDecoder for CacheDecoder<'a, 'tcx> {
    fn decode_syntax_context(&mut self) -> SyntaxContext {
        let syntax_contexts = self.syntax_contexts;
        rustc_span::hygiene::decode_syntax_context(self, self.hygiene_context, |this, id| {
            // This closure is invoked if we haven't already decoded the data for the `SyntaxContext` we are deserializing.
            // We look up the position of the associated `SyntaxData` and decode it.
            let pos = syntax_contexts.get(&id).unwrap();
            this.with_position(pos.to_usize(), |decoder| {
                let data: SyntaxContextKey = decode_tagged(decoder, TAG_SYNTAX_CONTEXT);
                data
            })
        })
    }

    fn decode_expn_id(&mut self) -> ExpnId {
        let hash = ExpnHash::decode(self);
        if hash.is_root() {
            return ExpnId::root();
        }

        if let Some(expn_id) = ExpnId::from_hash(hash) {
            return expn_id;
        }

        let krate = self.tcx.crate_num_from_incremental_id(hash.stable_crate_id());

        let expn_id = if krate == LOCAL_CRATE {
            // We look up the position of the associated `ExpnData` and decode it.
            let pos = self
                .expn_data
                .get(&hash)
                .unwrap_or_else(|| panic!("Bad hash {:?} (map {:?})", hash, self.expn_data));

            let data: ExpnData =
                self.with_position(pos.to_usize(), |decoder| decode_tagged(decoder, TAG_EXPN_DATA));

            // Another decoder may register this expansion while we decode it. Validate the data
            // before registration so that the locked identity check can safely reuse its result.
            // Hashing must stay outside the hygiene lock because parent expansions need that lock.
            self.tcx.with_untracked_stable_hashing_context(|hcx| {
                rustc_span::hygiene::register_cached_local_expn_id(data, hash, hcx)
            })
        } else {
            let index_guess = self.foreign_expn_data[&hash];
            self.tcx.expn_hash_to_expn_id(krate, index_guess, hash)
        };

        debug_assert_eq!(expn_id.krate, krate);
        expn_id
    }

    fn decode_span(&mut self) -> Span {
        let ctxt = SyntaxContext::decode(self);
        let parent = Option::<LocalDefId>::decode(self);
        let tag: u8 = Decodable::decode(self);

        let (lo, hi) = match tag {
            TAG_PARTIAL_SPAN => (BytePos(0), BytePos(0)),
            TAG_RELATIVE_SPAN => {
                let dlo = u32::decode(self);
                let dto = u32::decode(self);

                let enclosing = self.tcx.source_span_untracked(parent.unwrap()).data_untracked();
                (
                    BytePos(enclosing.lo.0.wrapping_add(dlo)),
                    BytePos(enclosing.lo.0.wrapping_add(dto)),
                )
            }
            TAG_FULL_SPAN => {
                let file_lo_index = SourceFileIndex::decode(self);
                let line_lo = usize::decode(self);
                let col_lo = RelativeBytePos::decode(self);
                let len = BytePos::decode(self);

                let file_lo = self.file_index_to_file(file_lo_index);
                let lo = file_lo.lines()[line_lo - 1] + col_lo;
                let lo = file_lo.absolute_position(lo);
                let hi = lo + len;
                (lo, hi)
            }
            _ => unreachable!(),
        };

        Span::new(lo, hi, ctxt, parent)
    }

    fn decode_crate_num(&mut self) -> CrateNum {
        let stable_id = StableCrateId::decode(self);
        let cnum = self.tcx.crate_num_from_incremental_id(stable_id);
        cnum
    }

    // Both the `CrateNum` and the `DefIndex` of a `DefId` can change in between two
    // compilation sessions. We use the `DefPathHash`, which is stable across
    // sessions, to map the old `DefId` to the new one.
    fn decode_def_id(&mut self) -> DefId {
        // Load the `DefPathHash` which is was we encoded the `DefId` as.
        let def_path_hash = DefPathHash::decode(self);

        // Using the `DefPathHash`, we can lookup the new `DefId`.
        // Subtle: We only encode a `DefId` as part of a query result.
        // If we get to this point, then all of the query inputs were green,
        // which means that the definition with this hash is guaranteed to
        // still exist in the current compilation session.
        match self.tcx.incremental_def_path_hash_to_def_id(def_path_hash) {
            Some(r) => r,
            None => panic!("Failed to convert DefPathHash {def_path_hash:?}"),
        }
    }

    fn decode_attr_id(&mut self) -> rustc_span::AttrId {
        panic!("cannot decode `AttrId` with `CacheDecoder`");
    }
}

impl<'a, 'tcx> BlobDecoder for CacheDecoder<'a, 'tcx> {
    fn decode_symbol(&mut self) -> Symbol {
        self.decode_symbol_or_byte_symbol(
            Symbol::new,
            |this| Symbol::intern(this.read_str()),
            |opaque| Symbol::intern(opaque.read_str()),
        )
    }

    fn decode_byte_symbol(&mut self) -> ByteSymbol {
        self.decode_symbol_or_byte_symbol(
            ByteSymbol::new,
            |this| ByteSymbol::intern(this.read_byte_str()),
            |opaque| ByteSymbol::intern(opaque.read_byte_str()),
        )
    }

    // This impl makes sure that we get a runtime error when we try decode a
    // `DefIndex` that is not contained in a `DefId`. Such a case would be problematic
    // because we would not know how to transform the `DefIndex` to the current
    // context.
    fn decode_def_index(&mut self) -> DefIndex {
        panic!("trying to decode `DefIndex` outside the context of a `DefId`")
    }
}

/// Implements [`Decodable`] for `&'tcx T`, where [`T: RefDecodable`](RefDecodable).
///
/// Due to orphan-rule restrictions, these foreign impls cannot use a blanket
/// [`D: TyDecoder`](TyDecoder), and must instead specify a specific decoder.
///
/// For impls on types defined in `rustc_middle`, see
/// `impl_decodable_via_ref_decodable_for_local_type!` instead.
macro_rules! impl_decodable_via_ref_decodable_for_foreign_types {
    (
        $(
            &'tcx $T:ty,
        )*
    ) => {
        $(
            impl<'tcx> Decodable<CacheDecoder<'_, 'tcx>> for &'tcx $T {
                fn decode(decoder: &mut CacheDecoder<'_, 'tcx>) -> Self {
                    RefDecodable::decode(decoder)
                }
            }
        )*
    }
}

impl_decodable_via_ref_decodable_for_foreign_types! {
    // tidy-alphabetical-start
    &'tcx DefIdSet,
    &'tcx FxIndexMap<LocalDefId, ty::DefinitionSiteHiddenType<'tcx>>,
    &'tcx IndexVec<mir::Promoted, mir::Body<'tcx>>,
    &'tcx UnordMap<DefId, ty::EarlyBinder<'tcx, Ty<'tcx>>>,
    &'tcx UnordSet<LocalDefId>,
    &'tcx [(
        rustc_middle::middle::exported_symbols::ExportedSymbol<'tcx>,
        rustc_middle::middle::exported_symbols::SymbolExportInfo,
    )],
    &'tcx [(ty::Clause<'tcx>, Span)],
    &'tcx [CodegenUnit<'tcx>],
    &'tcx [DefId],
    &'tcx [Spanned<MonoItem<'tcx>>],
    &'tcx [Symbol],
    &'tcx [crate::mono::UpstreamMono<'tcx>],
    &'tcx [rustc_middle::middle::dead_code::SymbolUse],
    &'tcx [ty::Variance],
    &'tcx rustc_ast::tokenstream::TokenStream,
    &'tcx rustc_middle::middle::dead_code::BodyUses<'tcx>,
    // tidy-alphabetical-end
}

//- ENCODING -------------------------------------------------------------------

/// An encoder that can write to the incremental compilation cache.
pub struct CacheEncoder<'tcx> {
    tcx: TyCtxt<'tcx>,
    encoder: FileEncoder<'static>,
    type_shorthands: FxHashMap<Ty<'tcx>, usize>,
    predicate_shorthands: FxHashMap<ty::PredicateKind<'tcx>, usize>,
    interpret_allocs: FxIndexSet<interpret::AllocId>,
    caching_source_map_view: CachingSourceMapView<'tcx>,
    file_to_file_index: FxHashMap<*const SourceFile, SourceFileIndex>,
    hygiene_context: Rc<RefCell<HygieneEncodeContext>>,
    // Used for both `Symbol`s and `ByteSymbol`s.
    symbol_index_table: FxHashMap<u32, usize>,

    query_values_index: Vec<(SerializedDepNodeIndex, AbsoluteBytePos)>,
    forwarded_queries: GrowableBitSet<SerializedDepNodeIndex>,
    side_effects_index: Vec<(SerializedDepNodeIndex, AbsoluteBytePos)>,
}

impl<'tcx> fmt::Debug for CacheEncoder<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Add more details here if/when necessary.
        f.write_str("CacheEncoder")
    }
}

impl<'tcx> CacheEncoder<'tcx> {
    #[inline]
    fn source_file_index(&mut self, source_file: Arc<SourceFile>) -> SourceFileIndex {
        self.file_to_file_index[&(&raw const *source_file)]
    }

    /// Encode something with additional information that allows to do some
    /// sanity checks when decoding the data again. This method will first
    /// encode the specified tag, then the given value, then the number of
    /// bytes taken up by tag and value. On decoding, we can then verify that
    /// we get the expected tag and read the expected number of bytes.
    fn encode_tagged<T: Encodable<Self>, V: Encodable<Self>>(&mut self, tag: T, value: &V) {
        let start_pos = self.position();

        tag.encode(self);
        value.encode(self);

        let end_pos = self.position();
        ((end_pos - start_pos) as u64).encode(self);
    }

    pub fn encode_query_value<V: Encodable<Self>>(&mut self, index: DepNodeIndex, value: &V) {
        let index = SerializedDepNodeIndex::from_curr_for_serialization(index);
        if self.forwarded_queries.contains(index) {
            return;
        }

        self.query_values_index.push((index, AbsoluteBytePos::new(self.position())));
        self.encode_tagged(index, value);
    }

    fn encode_side_effect(&mut self, index: DepNodeIndex, side_effect: &QuerySideEffect) {
        let index = SerializedDepNodeIndex::from_curr_for_serialization(index);

        self.side_effects_index.push((index, AbsoluteBytePos::new(self.position())));
        self.encode_tagged(index, side_effect);
    }

    // copy&paste impl from rustc_metadata
    fn encode_symbol_or_byte_symbol(
        &mut self,
        index: u32,
        emit_str_or_byte_str: impl Fn(&mut Self),
    ) {
        // if symbol/byte symbol is predefined, emit tag and symbol index
        if Symbol::is_predefined(index) {
            self.encoder.emit_u8(SYMBOL_PREDEFINED);
            self.encoder.emit_u32(index);
        } else {
            // otherwise write it as string or as offset to it
            match self.symbol_index_table.entry(index) {
                Entry::Vacant(o) => {
                    self.encoder.emit_u8(SYMBOL_STR);
                    let pos = self.encoder.position();
                    o.insert(pos);
                    emit_str_or_byte_str(self);
                }
                Entry::Occupied(o) => {
                    let x = *o.get();
                    self.emit_u8(SYMBOL_OFFSET);
                    self.emit_usize(x);
                }
            }
        }
    }

    #[inline]
    fn finish(mut self) -> FileEncodeResult {
        self.encoder.finish()
    }
}

impl<'tcx> SpanEncoder for CacheEncoder<'tcx> {
    fn encode_syntax_context(&mut self, syntax_context: SyntaxContext) {
        let idx = self.hygiene_context.borrow_mut().get_syntax_ctxt_encoding_index(syntax_context);
        idx.encode(self);
    }

    fn encode_expn_id(&mut self, expn_id: ExpnId) {
        self.hygiene_context.borrow_mut().schedule_expn_data_for_encoding(expn_id);
        expn_id.incremental_expn_hash().encode(self);
    }

    fn encode_span(&mut self, span: Span) {
        let span_data = span.data_untracked();
        span_data.ctxt.encode(self);
        span_data.parent.encode(self);

        if span_data.is_dummy() {
            return TAG_PARTIAL_SPAN.encode(self);
        }

        let parent =
            span_data.parent.map(|parent| self.tcx.source_span_untracked(parent).data_untracked());
        if let Some(parent) = parent
            && parent.contains(span_data)
        {
            TAG_RELATIVE_SPAN.encode(self);
            (span_data.lo.0.wrapping_sub(parent.lo.0)).encode(self);
            (span_data.hi.0.wrapping_sub(parent.lo.0)).encode(self);
            return;
        }

        let Some((file_lo, line_lo, col_lo)) =
            self.caching_source_map_view.byte_pos_to_line_and_col(span_data.lo)
        else {
            return TAG_PARTIAL_SPAN.encode(self);
        };

        if let Some(parent) = parent
            && file_lo.contains(parent.lo)
        {
            TAG_RELATIVE_SPAN.encode(self);
            (span_data.lo.0.wrapping_sub(parent.lo.0)).encode(self);
            (span_data.hi.0.wrapping_sub(parent.lo.0)).encode(self);
            return;
        }

        let len = span_data.hi - span_data.lo;
        let source_file_index = self.source_file_index(file_lo);

        TAG_FULL_SPAN.encode(self);
        source_file_index.encode(self);
        line_lo.encode(self);
        col_lo.encode(self);
        len.encode(self);
    }

    fn encode_symbol(&mut self, sym: Symbol) {
        self.encode_symbol_or_byte_symbol(sym.as_u32(), |this| this.emit_str(sym.as_str()));
    }

    fn encode_byte_symbol(&mut self, byte_sym: ByteSymbol) {
        self.encode_symbol_or_byte_symbol(byte_sym.as_u32(), |this| {
            this.emit_byte_str(byte_sym.as_byte_str())
        });
    }

    fn encode_crate_num(&mut self, crate_num: CrateNum) {
        self.tcx.incremental_crate_id(crate_num).encode(self);
    }

    fn encode_def_id(&mut self, def_id: DefId) {
        self.tcx.incremental_def_path_hash(def_id).encode(self);
    }

    fn encode_def_index(&mut self, _def_index: DefIndex) {
        bug!("encoding `DefIndex` without context");
    }
}

impl<'tcx> TyEncoder<'tcx> for CacheEncoder<'tcx> {
    const CLEAR_CROSS_CRATE: bool = false;

    #[inline]
    fn position(&self) -> usize {
        self.encoder.position()
    }
    #[inline]
    fn type_shorthands(&mut self) -> &mut FxHashMap<Ty<'tcx>, usize> {
        &mut self.type_shorthands
    }
    #[inline]
    fn predicate_shorthands(&mut self) -> &mut FxHashMap<ty::PredicateKind<'tcx>, usize> {
        &mut self.predicate_shorthands
    }
    #[inline]
    fn encode_alloc_id(&mut self, alloc_id: &interpret::AllocId) {
        let (index, _) = self.interpret_allocs.insert_full(*alloc_id);

        index.encode(self);
    }
}

macro_rules! encoder_methods {
    ($($name:ident($ty:ty);)*) => {
        #[inline]
        $(fn $name(&mut self, value: $ty) {
            self.encoder.$name(value)
        })*
    }
}

impl<'tcx> Encoder for CacheEncoder<'tcx> {
    encoder_methods! {
        emit_usize(usize);
        emit_u128(u128);
        emit_u64(u64);
        emit_u32(u32);
        emit_u16(u16);
        emit_u8(u8);

        emit_isize(isize);
        emit_i128(i128);
        emit_i64(i64);
        emit_i32(i32);
        emit_i16(i16);

        emit_raw_bytes(&[u8]);
    }
}

// This ensures that the `Encodable<opaque::FileEncoder>::encode` specialization for byte slices
// is used when a `CacheEncoder` having an `opaque::FileEncoder` is passed to `Encodable::encode`.
// Unfortunately, we have to manually opt into specializations this way, given how `CacheEncoder`
// and the encoding traits currently work.
impl<'tcx> Encodable<CacheEncoder<'tcx>> for [u8] {
    fn encode(&self, e: &mut CacheEncoder<'tcx>) {
        self.encode(&mut e.encoder);
    }
}

impl<'a, 'tcx> Decodable<CacheDecoder<'a, 'tcx>> for &'tcx [u8] {
    fn decode(decoder: &mut CacheDecoder<'a, 'tcx>) -> Self {
        let tcx = decoder.tcx;
        let len = decoder.read_usize();
        tcx.arena.alloc_slice(decoder.read_raw_bytes(len))
    }
}
