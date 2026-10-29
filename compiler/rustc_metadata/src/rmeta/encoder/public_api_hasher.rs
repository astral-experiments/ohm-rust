//! Computation of a crate's *public API hash*
//!
//! Public api hashing only runs when the unstable `-Zpublic-api-hash` option is enabled. The
//! crate root records whether it was (see `public_api_hash_opt_enabled` / the
//! `public_api_hash_opt_enabled` field on `CrateRoot`), so the decoder can fall back to the old
//! behaviour for rmetas produced without the option.
//!
//! # Public API hash
//! The goal of public API hashing is to allow skipping recompiles of downstream dependencies when only
//! private items change. It is a hash over everything in a crate that is observable by downstream
//! crates during a successful compilation through its rmeta. It is designed to have three properties:
//!  1. The public hash does not change when modifying private items. (With the caveat that changing
//!     the span of a public item by inserting a private item before it will change the public
//!     hash. Nevertheless, if private code is kept in separate files from public ones, or just
//!     added at the end of a file, it works.)
//!  2. Changing a public item in a dependency does not change the public hash of the local crate,
//!     unless that public item is reexported here. This allows recompilation to be cut off two
//!     crates downstream when the public hash of a crate changes, instead of propagating that all
//!     the way and requiring recompiles everywhere.
//!  3. It is future proof: new data added to the rmeta is included in the hash by default rather
//!     than silently left out.
//!
//! NOTE: *private* here means not usable by downstream deps. Some "private" data can leak through
//! inlinable functions, const evaluatable functions and many more things. The next section has a
//! more in-depth explanation about how the public API hash is calculated.
//!
//! # How this is achieved reliably
//!
//! _For simplicity, this explanation mostly uses [`DefId`]s as ids. There are a few other kinds of
//! ids, like [`ExpnId`], but the mechanism is the same.
//!
//! Let's start with a simple observation: rmeta does not provide queries like `all_def_ids`
//! that iterate over every def id in the crate. It stores most data in tables that can only be
//! accessed by calling queries which require a `DefId` as input and then return some output. So
//! most information is only accessible through maps that map a `DefId` to some data — with a few
//! exceptions.
//! This allows us to think of the metadata as a graph where we can start from a `DefId` use all
//! available queries to get all data associated with it, then collect all other `DefId`-s in the
//! associated data. Those `DefId`-s are immediately reachable from our starting id. Repeat it for
//! all newly found `DefId`-s and we have a graph of what is reachable from which `DefId`. This
//! is what we will call the reachability graph. Start from the crate root and the "few exceptions"
//! mentioned above and we get all publicly reachable items.
//!
//! The reachability graph is constructed during encoding: whenever an `id -> data` mapping is
//! encoded into rmeta, every id encoded together with `data` is recorded as an edge from `id`. For
//! example, if `id` is a public function we always add edges to its span and to the types in its
//! signature; if it also has `mir` encoded, we add all the types/functions/spans occurring inside.
//! For any associated data stored in tables, encoder callbacks make sure those edges automatically
//! appear in the graph. This is how we ensure property 3: new data added to the rmeta is included
//! in the hash by default.
//!
//! Once the graph is constructed, each node has a local hash: the combined hash of all the data
//! saved for it in these tables. The strongly connected components are then computed and each scc
//! is assigned a public hash: the local hash of everything inside the connected component plus the
//! public hash from each of its dependencies.
//!
//! Finally, each reachable [`LocalDefId`] has its public hash (the public hash of its scc) saved
//! into rmeta, which enables property 2. The public hash of a crate does not depend on the full
//! public hash of its dependencies, only on their global part (like lang items and externally
//! implementable items) plus the hash of the items that are reachable through this rmeta.
//!
//! # Modifications to how some data is stored in the rmeta
//! The storage of some data is not compatible with public api hashing. The following changes are
//! made when the option is enabled:
//!
//! ## Encoding the `DefIndex` of a `DefId` as a stable `DefPathHash`
//!
//! A `DefId` is normally encoded as a `(CrateNum, DefIndex)` pair, where `DefIndex` is just an
//! allocation-order index into the crate. That index shifts whenever definitions are added or
//! reordered — including private ones — so hashing it directly would break property 1. When
//! public-api hashing is enabled, the `DefIndex` part is instead encoded as the definition's stable
//! `DefPathHash`, which only depends on the def path and not on allocation order. This is what makes
//! it possible to keep `def_path_hash_map` out of the public hash, and is also why trait impls are
//! stored keyed by `DefPathHash` (see `EncodedTraitImpls`) rather than by `DefIndex`.
//!
//! ## Converting per-`DefId` tables instead of whole-crate queries
//!
//! To ensure properties 1 and 2, some information that used to be exposed as a single whole-crate
//! query — returning a set or map over every `DefId` in the crate — breaks this in two ways: such
//! an aggregate is not keyed by `DefId`, so it cannot be placed in the reachability graph at all,
//! and it changes whenever *any* item (including a private one) is added to or removed from the
//! set.
//!
//! So when public-api hashing is enabled, these aggregates are replaced by per-`DefId`
//! tables, and the original whole-crate queries are asserted to be unused (see the asserts in the
//! decoder's `reachable_non_generics` / `exportable_items` providers):
//!  - `exportable_items` (a set) becomes the `is_exportable` query, backed by the `is_exportable`
//!    table.
//!  - `reachable_non_generics` (a map) becomes the `is_reachable_non_generic` query, backed by the
//!    `is_reachable_non_generic` table, plus the `is_reachable_non_generic_with_export_level_c`
//!    query/table for the export-level information that map also carried.
//!
//! Because they are now `DefId -> bool` tables they participate in the reachability graph like any
//! other encoded data, so only the entries of reachable items contribute to the hash.
//!
//! # What is moved out of the public hash
//!
//! Some rmeta contents are deliberately not part of the public hash, because they are only needed
//! at link time or only for diagnostics, and including them would couple the public hash to private
//! implementation details.
//!  - `exported_non_generic_symbols` are only used during linking, so they live behind the private
//!    hash ([`Unhashed`]). Exported generic symbols remain in the public hash because downstream
//!    crates can reuse their monomorphizations; changing the available symbols must invalidate
//!    those dependents.
//!  - `def_path_hash_map` is unhashed (see above).
//!  - `traits` and `stripped_cfg_items` are only read when a compilation session ends in an error,
//!    so they are currently dropped from the rmeta when the option is enabled.
//!  - Proc-macro crates are not graph-hashed at all; their full crate hash is used as the public
//!    hash.
//!
//! # Tooling and testing
//!
//! - `-Zls=public_hash` prints the public hash of a metadata file.
//! - The `rustc_public_hash_changed` / `rustc_public_hash_unchanged` test attributes let
//!   incremental tests assert whether a change moved a dependency's public hash.
//!
//! # Improving it safely
//!
//! The graph constructed from the rmeta has redundant edges. A `DefId` occurring in the result of
//! a query does not mean it is actually used to access all data reachable through that `DefId`.
//!
//! An example of this is how `ExpnData` was changed: its `parent_module` field is only used to
//! check visibilities, never as an input to a query, and its `macro_def_id` field is set to
//! `LOCAL_CRATE` for the root expansion, but that is only used in queries when the returned
//! `DefId` is that of a macro. Changing their types to typed `DefId`s (the `VisibilityDefId`
//! newtype) marks them as visibility-only, allowing the encoder to omit those edges from the rmeta
//! reachability graph. Callers must uphold that marker's contract and not use its inner `DefId` as
//! a metadata query key.

use core::iter::Iterator;
use std::cell::RefCell;
use std::fmt;

use rustc_crate_store::{ForeignModule, LinkagePreference, NativeLib};
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::fx::{FxHashSet, FxIndexSet};
use rustc_data_structures::graph::scc::Sccs;
use rustc_data_structures::graph::{DirectedGraph, Successors};
use rustc_data_structures::sso::SsoHashSet;
use rustc_data_structures::stable_hash::{StableHash, StableHashCtxt, StableHasher};
use rustc_data_structures::svh::Svh;
use rustc_data_structures::unord::UnordMap;
use rustc_hir::attrs::StrippedCfgItem;
use rustc_hir::attrs::lang_items::LangItem;
use rustc_hir::def_id::DefIndex;
use rustc_index::{Idx, IndexVec};
use rustc_macros::{LazyDecodable, MetadataEncodable, StableHash};
use rustc_middle::ich::StableHashState;
use rustc_middle::middle::debugger_visualizer::DebuggerVisualizerFile;
use rustc_middle::middle::exported_symbols::{ExportedSymbol, SymbolExportInfo};
use rustc_middle::middle::lib_features::FeatureStability;
use rustc_middle::mir::interpret::AllocId;
use rustc_middle::ty::{PredicateKind, Ty, TyCtxt};
use rustc_session::config::mitigation_coverage::DeniedPartialMitigation;
use rustc_session::config::{SymbolManglingVersion, TargetModifier};
use rustc_span::def_id::{CRATE_DEF_ID, DefId, LOCAL_CRATE, LocalDefId, StableCrateId};
use rustc_span::edition::Edition;
use rustc_span::hygiene::ExpnIndex;
use rustc_span::{ExpnId, LocalExpnId, Span, Symbol, SyntaxContext};
use rustc_target::spec::PanicStrategy;
use tracing::debug;

use super::{CrateDep, CrateHeader, CrateRoot, TargetTuple};
use crate::rmeta::encoder::{DefPathHashDefIdEncoder, MetadataEncoder};
use crate::rmeta::{
    CrateHashes, DefPathHashMapRef, EiiMapEncodedKeyValue, EncodeContext, EncodedTraitImpls,
    ExpnDataTable, ExpnHashTable, IncoherentImpls, LazyArray, LazyDecoder, LazyTable, LazyTables,
    LazyValue, ProcMacroData, SyntaxContextTable, TableBuilder,
};

/// Accumulates a fingerprint only while public API hashing is enabled.
#[derive(Default)]
pub(crate) struct PublicApiHasher(StableHasher);

impl PublicApiHasher {
    #[inline(always)]
    pub(crate) fn finish<'a>(self, hcx: &mut impl PublicApiHashState<'a>) -> Option<Fingerprint> {
        hcx.enabled().then(|| self.0.finish())
    }

    #[inline(always)]
    pub(crate) fn digest<'a, T: StableHash>(
        &mut self,
        value: T,
        hcx: &mut impl PublicApiHashState<'a>,
    ) {
        if hcx.enabled() {
            value.stable_hash(hcx.hcx_mut(), &mut self.0);
        }
    }
}

/// Selects whether encoded table entries contribute hashes to their reachability-graph nodes.
pub(crate) trait TablePublicApiHasher<I: Idx>: Default {
    type IterHasher;

    fn digest<'a, V>(
        &mut self,
        index: impl TableIndex<Encoded = I>,
        value: V,
        hcx: &mut impl PublicApiHashState<'a>,
    ) where
        V: StableHash;

    fn iter_hasher(&self) -> Self::IterHasher;
}

/// Records every encoded table entry in the public API reachability graph.
pub(crate) struct RDRHashAll<I: Idx> {
    _marker: std::marker::PhantomData<I>,
}

impl<I: Idx> Default for RDRHashAll<I> {
    fn default() -> Self {
        Self { _marker: Default::default() }
    }
}

/// Provides hashing state shared by metadata encoding and reachability-graph construction.
pub(crate) trait PublicApiHashState<'a> {
    fn enabled(&self) -> bool;

    fn hcx_mut(&mut self) -> &mut StableHashState<'a, true>;

    fn add_node_hash(&mut self, node: &Node<'a>, hash: Fingerprint);

    fn hcx_with_def_id_hashes(
        &mut self,
    ) -> (&mut StableHashState<'a, true>, &ReachabilityGraphHashes);
}

impl<'a, const ENABLED: bool> PublicApiHashState<'a> for PublicApiHashingContext<'a, ENABLED> {
    #[inline(always)]
    fn enabled(&self) -> bool {
        ENABLED
    }

    #[inline(always)]
    fn hcx_mut(&mut self) -> &mut StableHashState<'a, true> {
        &mut self.hcx
    }

    #[inline(always)]
    fn add_node_hash(&mut self, node: &Node<'a>, hash: Fingerprint) {
        if self.enabled() {
            self.def_id_hashes.add_node_hash(node, hash);
        }
    }

    fn hcx_with_def_id_hashes(
        &mut self,
    ) -> (&mut StableHashState<'a, true>, &ReachabilityGraphHashes) {
        (&mut self.hcx, &self.def_id_hashes)
    }
}

/// Compiles metadata encoding with public API hashing either enabled or disabled.
pub(crate) struct PublicApiHashingContext<'a, const ENABLED: bool> {
    pub(crate) hcx: StableHashState<'a, true>,
    def_id_hashes: ReachabilityGraphHashes,
}

impl<'a, const ENABLED: bool> PublicApiHashingContext<'a, ENABLED> {
    pub(crate) fn new(hcx: StableHashState<'a, false>) -> Self {
        Self { hcx: hcx.hash_spans_as_parentless(), def_id_hashes: Default::default() }
    }
}

impl<I: Idx> TablePublicApiHasher<I> for RDRHashAll<I> {
    type IterHasher = OrderedIterHasher;
    #[inline(always)]
    fn digest<'a, V>(
        &mut self,
        index: impl TableIndex<Encoded = I>,
        value: V,
        hcx: &mut impl PublicApiHashState<'a>,
    ) where
        V: StableHash,
    {
        if !hcx.enabled() {
            return;
        }
        let mut hasher = StableHasher::default();
        value.stable_hash(hcx.hcx_mut(), &mut hasher);
        hcx.add_node_hash(&index.into(), hasher.finish());
    }

    #[inline(always)]
    fn iter_hasher(&self) -> Self::IterHasher {
        OrderedIterHasher::default()
    }
}

/// Hashes an iterator in encoding order without collecting it first.
#[derive(Default)]
pub(crate) struct OrderedIterHasher(StableHasher);

impl OrderedIterHasher {
    #[inline(always)]
    pub(crate) fn inspect_digest<'a: 'b, 'b, I>(
        &'b mut self,
        iter: I,
        hcx: &'b mut impl PublicApiHashState<'a>,
    ) -> impl Iterator<Item = I::Item> + 'b
    where
        I: IntoIterator + 'b,
        I::Item: StableHash,
    {
        iter.into_iter().inspect(move |item: &I::Item| {
            if hcx.enabled() {
                item.stable_hash(hcx.hcx_mut(), &mut self.0)
            }
        })
    }

    #[inline(always)]
    pub(crate) fn finish(self) -> Fingerprint {
        self.0.finish()
    }
}

/// Encodes table entries without contributing them to the public API hash.
pub(crate) struct RDRHashNone<I>(std::marker::PhantomData<I>);

impl<I> Default for RDRHashNone<I> {
    fn default() -> Self {
        RDRHashNone(Default::default())
    }
}

impl<I: Idx> TablePublicApiHasher<I> for RDRHashNone<I> {
    type IterHasher = RDRHashNone<()>;
    #[inline(always)]
    fn digest<'a, V>(
        &mut self,
        _index: impl TableIndex<Encoded = I>,
        _value: V,
        _hcx: &mut impl PublicApiHashState<'a>,
    ) where
        V: StableHash,
    {
    }

    #[inline(always)]
    fn iter_hasher(&self) -> Self::IterHasher {
        Default::default()
    }
}

/// Marks a value whose public-hash contribution is already stored in [`ReachabilityGraphHashes`].
///
/// Its [`StableHash`] implementation intentionally emits nothing.
#[derive(Default)]
pub(crate) struct GraphHashed<T>(pub(crate) T);
impl<T> StableHash for GraphHashed<T> {
    fn stable_hash<Hcx: StableHashCtxt>(&self, _hcx: &mut Hcx, _hasher: &mut StableHasher) {}
}

impl<T> fmt::Debug for GraphHashed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GraphHashed")
    }
}

/// Marks metadata intentionally excluded from the public API hash.
#[derive(Default)]
pub(crate) struct Unhashed<T>(pub(crate) T);
impl<T> StableHash for Unhashed<T> {
    fn stable_hash<Hcx: StableHashCtxt>(&self, _hcx: &mut Hcx, _hasher: &mut StableHasher) {}
}

impl<T> fmt::Debug for Unhashed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "unhashed".fmt(f)
    }
}

/// Carries an encoded value and the fingerprint computed while encoding it.
///
/// `hash` must be present whenever this wrapper is formatted or stably hashed.
#[derive(Default)]
pub(crate) struct Hashed<T> {
    pub(crate) hash: Option<Fingerprint>,
    pub(crate) value: T,
}

impl<T> fmt::Debug for Hashed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.hash.as_ref().unwrap().fmt(f)
    }
}

impl<T> StableHash for Hashed<T> {
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        self.hash.expect("Hash must be present when stable hashing!").stable_hash(hcx, hasher);
    }
}

/// Carries legacy metadata that must be absent whenever public API hashing is active.
pub(crate) struct NoneIfHashed<T> {
    pub(super) value: Option<T>,
}

impl<T> fmt::Debug for NoneIfHashed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        None::<()>.fmt(f)
    }
}

impl<T> StableHash for NoneIfHashed<T> {
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        assert!(self.value.is_none());
        0u32.stable_hash(hcx, hasher);
    }
}

#[derive(StableHash, Debug)]
pub(crate) struct HashableCrateHeader {
    pub(crate) triple: TargetTuple,
    pub(crate) name: Symbol,
    pub(crate) is_proc_macro_crate: bool,
    pub(crate) is_stub: bool,
}

/// Stable-hashable version of [`CrateRoot`] used to calculate the public API hash. When adding
/// new fields to `CrateRoot`, it is important to include them in the public API hash. Not doing so
/// can cause silent miscompiles. This struct helps make sure the compiler does not allow forgetting
/// the hashing of new items added to rmeta.
///
/// When adding a new field to `CrateRoot` we have 3 cases: it is encoded directly, bools for
/// example, it is added as a `LazyValue` or `LazyArray`, or it is added as a new table to
/// `LazyTables`
/// 1. When encoded directly, the type likely implements `StableHash` or it can be derived for it.
///    Simply adding it as-is to `HashableCrateRoot` then moving it into `CrateRoot` in
///    `into_crate_root` should do the trick.
/// 2. When encoded as some kind of lazy value, if one tries to do the same as in 1, the compiler
///    will complain that it does not implement `StableHash`, so it cannot derive
///    `StableHash` for `HashableCrateRoot`. In this case one should wrap it in [`Hashed`] and hash
///    it where it was encoded. The `hashed_lazy_array!` macro can help with this for simple arrays.
/// 3. When added to `LazyTables` in the `define_tables!` macro, simply defining its type as
///    `Table<RDRHashAll, Idx, Value>` will take care of hashing it.
///
/// In some special cases, the hash of the field might not be needed at all in the public API
/// hash. If it is only used for better error reporting, or only during link time, this is likely
/// the case. Use the [`Unhashed`] wrapper to skip a field from the public hash, or the
/// [`GraphHashed`] wrapper to indicate that it was hashed with the reachability graph.
#[derive(StableHash, Debug)]
pub(crate) struct HashableCrateRoot {
    // =========== not reviewed ============
    // These were not reviewed for public API hashing, so they are included in the global hash
    // for now. Any change in these fields will cause a recompile of all downstream dependencies.
    pub(crate) header: HashableCrateHeader,
    pub(crate) required_panic_strategy: Option<PanicStrategy>,
    pub(crate) panic_in_drop_strategy: PanicStrategy,
    pub(crate) dylib_dependency_formats: Hashed<LazyArray<Option<LinkagePreference>>>,
    pub(crate) lib_features: Hashed<LazyArray<(Symbol, FeatureStability)>>,
    pub(crate) stability_implications: Hashed<LazyArray<(Symbol, Symbol)>>,
    // I think this is mostly used in the std, so it doesn't matter much whether we include it
    pub(crate) diagnostic_items: Hashed<LazyArray<(Symbol, DefIndex)>>,
    pub(crate) canonical_symbols: Hashed<LazyArray<(Symbol, DefIndex)>>,
    pub(crate) native_libraries: Hashed<LazyArray<NativeLib>>,
    pub(crate) foreign_modules: Hashed<LazyArray<ForeignModule>>,
    pub(crate) incoherent_impls: Hashed<LazyArray<IncoherentImpls>>,
    pub(crate) debugger_visualizers: Hashed<LazyArray<DebuggerVisualizerFile>>,
    pub(crate) stable_order_of_exportable_impls: Hashed<LazyArray<(DefIndex, usize)>>,
    pub(crate) target_modifiers: Hashed<LazyArray<TargetModifier>>,
    pub(crate) denied_partial_mitigations: Hashed<LazyArray<DeniedPartialMitigation>>,
    pub(crate) compiler_builtins: bool,
    pub(crate) needs_allocator: bool,
    pub(crate) needs_panic_runtime: bool,
    pub(crate) no_builtins: bool,
    pub(crate) panic_runtime: bool,
    pub(crate) profiler_runtime: bool,
    pub(crate) symbol_mangling_version: SymbolManglingVersion,

    pub(crate) specialization_enabled_in: bool,
    pub(crate) public_api_hash_opt_enabled: bool,

    // =========== global hash  ============
    // Any change in these fields will cause a recompile of all downstream dependencies

    // For the next 4, there can only be a single one globally
    pub(crate) has_global_allocator: bool,
    pub(crate) has_alloc_error_handler: bool,
    pub(crate) has_panic_handler: bool,
    pub(crate) has_default_lib_allocator: bool,

    // extra_filenames and stable_crate_id must be in the global hash since these are needed
    // to differentiate between different version of the same crate. Different versions might have
    // the same public api, but we must keep them separate, as the user might be using different
    // versions exactly because their private implementations differ.
    pub(crate) extra_filename: String,
    pub(crate) stable_crate_id: StableCrateId,

    // macro expansion in downstream crates uses it, only relevant for publicly reachable macros,
    // FIXME: This is left here as it is unlikely to change much between compilations. But we
    // could move this to macro hashes and remove it from the global hash.
    pub(crate) edition: Edition,
    // Changing externally implementable items should cause recompiles in all downstream crates as
    // there can only be a single one of each eii globally.
    // FIXME EiiDecl and EiiImpl contain spans. Should changing the span of these items cause
    // recompiles?
    // FIXME eii-s are collected in `rustc_metadata::eii::collect`. We should probably stable sort
    // that there to make the query result more stable (but sorting might be useless if this
    // should be sensitive to span changes)
    pub(crate) externally_implementable_items: Hashed<LazyArray<EiiMapEncodedKeyValue>>,

    // lang items are checked for uniqueness globally
    pub(crate) lang_items: Hashed<LazyArray<(DefIndex, LangItem)>>,
    // FIXME: Should we exclude this? `lang_items` of this crate and the global hash of
    // dependencies should already cover it
    pub(crate) lang_items_missing: Hashed<LazyArray<LangItem>>,

    // We need the global hashes from the dependencies included in the global hash of this
    // crate
    pub(crate) crate_deps: Hashed<LazyArray<CrateDep>>,

    // =========== graph hashed ============

    // See the tables definition for what is hashed. Hashed tables are interpreted as
    // `LocalDefId -> data` maps and are included in the reachability graph
    pub(crate) tables: GraphHashed<LazyTables>,

    // The relevant parts of the source code are hashed when the spans are hashed as reachability
    // graph nodes.
    pub(crate) source_map: GraphHashed<LazyTable<u32, Option<LazyValue<rustc_span::SourceFile>>>>,

    // Macro expansion data
    pub(crate) syntax_contexts: GraphHashed<SyntaxContextTable>,
    pub(crate) expn_data: GraphHashed<ExpnDataTable>,
    pub(crate) expn_hashes: GraphHashed<ExpnHashTable>,

    // Mir interpreter allocations for const eval. Haven't looked too much into this, but the
    // allocations can include types (and other stuff?), so we include this in the reachability
    // graph to be sure.
    // FIXME Does stable hashing the AllocId already include the stable hash of everything we
    // need for the reachability graph? We might not need to include these explicitly as
    // reachability graph nodes.
    pub(crate) interpret_alloc_index: GraphHashed<LazyArray<u64>>,

    // this is exposed as a full query with `trait_impls_of_crate`. Which is only used in
    // rustc_public. Otherwise the `implementations_of_trait` is the only other consumer of this
    // field. `implementations_of_trait` works as a map. You need to provide a trait DefId to get
    // its impls, so this is integrated into the ReachabilityGraph.
    //
    // When a trait is local:
    //      if it isn't reachable, the impls should be left out from the hash
    // When a trait is from another crate:
    //      even if this crate does not reexport that trait, a downstream dependency can import it
    //      from another crate, then invoke its methods. So all of those impls must be included,
    //      but only the ones that can be applied to publicly reachable types
    pub(crate) impls: GraphHashed<EncodedTraitImpls>,

    // This is used to do symbol mangling in downstream crates. We should only include ones that
    // are somehow reachable.
    // While public api hashing is enabled, a table is used in the `is_exportable` field of
    // `tables` is used to provide this query. This field must be empty.
    pub(crate) exportable_items: GraphHashed<LazyArray<DefIndex>>,

    // =========== used during link time ==============
    // Only used during linking, so we can put this behind private hash.
    pub(crate) exported_non_generic_symbols:
        Unhashed<LazyArray<(ExportedSymbol<'static>, SymbolExportInfo)>>,

    // Downstream crates can reuse these monomorphizations during codegen. Their stable export set
    // therefore belongs in the public hash even when a private body produced an instance.
    pub(crate) exported_generic_symbols:
        Hashed<LazyArray<(ExportedSymbol<'static>, SymbolExportInfo)>>,
    pub(crate) has_generic_symbols_for_c_linker: bool,

    // =========== not needed in the public hash ==============
    // proc macro, ignored. We use the full crate hash as public hash for proc macros
    pub(crate) proc_macro_data: NoneIfHashed<ProcMacroData>,
    // The introduction of the option to store the DefIndex part of DefId-s as DefPathHash in the
    // metadata was done to remove this from the public hash.
    pub(crate) def_path_hash_map: Unhashed<LazyValue<DefPathHashMapRef<'static>>>,
    // Only used for diagnostics when the compilation session errors
    // FIXME: this is currently removed from the metadata when public api hashing is enabled. We
    // should add a safe way to access this when the compilation session errors.
    pub(crate) stripped_cfg_items: Unhashed<LazyArray<StrippedCfgItem<DefIndex>>>,

    // The traits defined in this crate.
    // According to the docs, and some personal digging, this is used by rustdoc and error reporting
    // error reporting: this should not be included in the public hash, as it is only read when the
    // compiler session finishes with an error. Just like stripped_cfg_items.
    // FIXME: this is currently removed from the metadata when public api hashing is enabled. We
    // should ensure that this is not misused before readding it.
    pub(crate) traits: Unhashed<LazyArray<DefIndex>>,
}

pub(super) fn crate_hashes<'tcx, 'h>(
    ecx: &mut EncodeContext<'_, 'tcx, DefPathHashDefIdEncoder<'tcx>>,
    hcx: &mut impl PublicApiHashState<'h>,
    root: &HashableCrateRoot,
) -> (CrateHashes, Option<ItemPublicHashes>) {
    assert!(!root.header.is_proc_macro_crate);
    let graph = std::mem::take(&mut ecx.spec_encoder_data.reachability_graph_builder)
        .build_graph(hcx.hcx_mut());
    debug!("Hash graph {graph:?}");
    let (hcx_mut, def_id_hashes) = hcx.hcx_with_def_id_hashes();
    let public_hashes = build_public_hashes(&graph, def_id_hashes, ecx.tcx, hcx_mut);

    // Reachable span text bypasses query tracking, so preserve its exact value
    // when reusing the encoded item prefix in a later session.
    let tcx = ecx.tcx;
    if let Some(manifest) = &mut ecx.metadata_prefix_manifest
        && graph.nodes.iter().any(
            |(node, _)| matches!(node, Node::Span(span) if !manifest.capture_protected_span(tcx, *span)),
        )
    {
        ecx.metadata_prefix_manifest = None;
    }

    let mut hasher = StableHasher::default();
    // A legacy intermediate can retain positional expansion IDs into a public dependency.
    // Prevent it from accepting a new-format producer under the old equivalence relation.
    let mut public_global_hash =
        stable_hash(hcx.hcx_mut(), &(crate::rmeta::OHM_METADATA_EPOCH, root));
    if tcx.sess.opts.unstable_opts.incremental_ignore_spans {
        // Per-item hashes omit spans in this mode. Invalidate readers on local source edits.
        public_global_hash =
            stable_hash(hcx.hcx_mut(), &(public_global_hash, tcx.crate_hash(LOCAL_CRATE)));
    }
    debug!("crate_hashes: public_global_hash = {public_global_hash:?}");
    debug!("crate_hashes: public_hashes roots hash = {:?}", public_hashes.hash);
    public_global_hash.stable_hash(hcx.hcx_mut(), &mut hasher);
    public_hashes.stable_hash(hcx.hcx_mut(), &mut hasher);
    let rdr_hashes = public_hashes.value.encode(ecx, public_global_hash);
    let public_hash = Svh::new(hasher.finish());
    debug!("Hashed crate root: {root:#x?}");
    debug!("public api hash: {}", public_hash);
    (CrateHashes { public_hash, private_hash: ecx.tcx.crate_hash(LOCAL_CRATE) }, Some(rdr_hashes))
}

impl HashableCrateRoot {
    pub(super) fn into_crate_root<'h, 'tcx, M: MetadataEncoder<'tcx>>(
        self,
        ecx: &mut EncodeContext<'_, 'tcx, M>,
        hcx: &mut impl PublicApiHashState<'h>,
    ) -> CrateRoot {
        let (hashes, rdr_hashes) = M::crate_hashes(ecx, hcx, &self);
        let header = self.header;
        let header = CrateHeader {
            triple: header.triple,
            hashes,
            name: header.name,
            is_proc_macro_crate: header.is_proc_macro_crate,
            is_stub: header.is_stub,
        };
        CrateRoot {
            header,

            extra_filename: self.extra_filename,
            stable_crate_id: self.stable_crate_id,
            required_panic_strategy: self.required_panic_strategy,
            panic_in_drop_strategy: self.panic_in_drop_strategy,
            edition: self.edition,
            has_global_allocator: self.has_global_allocator,
            has_alloc_error_handler: self.has_alloc_error_handler,
            has_panic_handler: self.has_panic_handler,
            has_default_lib_allocator: self.has_default_lib_allocator,
            externally_implementable_items: self.externally_implementable_items.value,

            crate_deps: self.crate_deps.value,
            dylib_dependency_formats: self.dylib_dependency_formats.value,
            lib_features: self.lib_features.value,
            stability_implications: self.stability_implications.value,
            lang_items: self.lang_items.value,
            lang_items_missing: self.lang_items_missing.value,
            stripped_cfg_items: self.stripped_cfg_items.0,
            diagnostic_items: self.diagnostic_items.value,
            canonical_symbols: self.canonical_symbols.value,
            native_libraries: self.native_libraries.value,
            foreign_modules: self.foreign_modules.value,
            traits: self.traits.0,
            impls: self.impls.0,
            incoherent_impls: self.incoherent_impls.value,
            interpret_alloc_index: self.interpret_alloc_index.0,
            proc_macro_data: self.proc_macro_data.value,

            tables: self.tables.0,
            debugger_visualizers: self.debugger_visualizers.value,

            exportable_items: self.exportable_items.0,
            stable_order_of_exportable_impls: self.stable_order_of_exportable_impls.value,
            exported_non_generic_symbols: self.exported_non_generic_symbols.0,
            exported_generic_symbols: self.exported_generic_symbols.value,
            has_generic_symbols_for_c_linker: self.has_generic_symbols_for_c_linker,

            syntax_contexts: self.syntax_contexts.0,
            expn_data: self.expn_data.0,
            expn_hashes: self.expn_hashes.0,

            def_path_hash_map: self.def_path_hash_map.0,

            source_map: self.source_map.0,
            target_modifiers: self.target_modifiers.value,
            denied_partial_mitigations: self.denied_partial_mitigations.value,

            compiler_builtins: self.compiler_builtins,
            needs_allocator: self.needs_allocator,
            needs_panic_runtime: self.needs_panic_runtime,
            no_builtins: self.no_builtins,
            panic_runtime: self.panic_runtime,
            profiler_runtime: self.profiler_runtime,
            symbol_mangling_version: self.symbol_mangling_version,

            specialization_enabled_in: self.specialization_enabled_in,
            rdr_hashes,
            public_api_hash_opt_enabled: self.public_api_hash_opt_enabled,
        }
    }
}

/// Controls how nodes encountered during nested metadata encoding enter the reachability graph.
pub(crate) enum RecordMode<'tcx> {
    /// Records nested nodes as outgoing edges from this node.
    From(Node<'tcx>),
    /// Suppresses nodes that are deliberately excluded from the public API graph.
    None,
}

struct ReachabilityGraph<'tcx> {
    nodes: Vec<(Node<'tcx>, Fingerprint)>,
    edges: IndexVec<NodeIdx, Vec<NodeIdx>>,
    roots: Vec<NodeIdx>,
}

impl fmt::Debug for ReachabilityGraph<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let node = |idx: NodeIdx| self.nodes[idx].0;
        let fingerprint = |idx: NodeIdx| self.nodes[idx].1;
        let reachable = self.reachable_set();
        writeln!(f, "roots:")?;
        for root in &self.roots {
            writeln!(f, "   {:?}", node(*root))?;
        }
        writeln!(f, "nodes:")?;
        for (from, edges) in self.edges.iter_enumerated() {
            writeln!(
                f,
                "{:?}: {:x?} reachable: {}",
                node(from),
                fingerprint(from),
                reachable[from]
            )?;
            for to in edges {
                writeln!(f, "   {:?}", node(*to))?;
            }
        }
        Ok(())
    }
}

impl<'tcx> ReachabilityGraph<'tcx> {
    fn reachable_set(&self) -> IndexVec<NodeIdx, bool> {
        let mut reachable = IndexVec::from_elem_n(false, self.edges.len());
        let mut stack = self.roots.clone();

        while let Some(node) = stack.pop() {
            if reachable[node] {
                continue;
            }
            reachable[node] = true;
            stack.extend_from_slice(&self.edges[node]);
        }

        reachable
    }
}

/// Collects semantic dependencies encountered while metadata is encoded.
pub(super) struct ReachabilityGraphBuilder<'tcx> {
    // `None` suppresses recording; an empty stack records roots.
    record_mode: Vec<Option<NodeIdx>>,
    nodes: FxIndexSet<Node<'tcx>>,
    edges: IndexVec<NodeIdx, SsoHashSet<NodeIdx>>,
    roots: FxHashSet<NodeIdx>,
    retained: FxHashSet<NodeIdx>,
}

impl Default for ReachabilityGraphBuilder<'_> {
    fn default() -> Self {
        let mut builder = Self {
            record_mode: Default::default(),
            nodes: Default::default(),
            edges: Default::default(),
            roots: Default::default(),
            retained: Default::default(),
        };
        let crate_root = builder.node_index(CRATE_DEF_ID.into());
        builder.roots.insert(crate_root);
        builder
    }
}

fn stable_hash<'a, T: StableHash + ?Sized, const HASH_SPANS_AS_PARENTLESS: bool>(
    hcx: &mut StableHashState<'a, HASH_SPANS_AS_PARENTLESS>,
    val: &T,
) -> Fingerprint {
    let mut hasher = StableHasher::new();
    val.stable_hash(hcx, &mut hasher);
    hasher.finish()
}

impl<'tcx> ReachabilityGraphBuilder<'tcx> {
    /// Interns an edge source once for all nodes encoded under this mode.
    pub(super) fn push_record_mode(&mut self, mode: RecordMode<'tcx>) {
        let from = match mode {
            RecordMode::From(node) => Some(self.node_index(node)),
            RecordMode::None => None,
        };
        self.record_mode.push(from);
    }

    pub(super) fn pop_record_mode(&mut self) {
        self.record_mode.pop();
    }

    /// Interns a semantic node so edges can store compact indices instead of full node values.
    fn node_index(&mut self, node: Node<'tcx>) -> NodeIdx {
        let (index, inserted) = self.nodes.insert_full(node);
        if inserted {
            let edge_index = self.edges.push(SsoHashSet::default());
            debug_assert_eq!(index, edge_index);
        }
        index
    }

    /// Keep the per-definition hash available to metadata readers without treating it as a crate root.
    pub(super) fn retain_public_hash(&mut self, node: Node<'tcx>) {
        let index = self.node_index(node);
        self.retained.insert(index);
    }

    fn build_graph(mut self, hcx: &mut StableHashState<'_, true>) -> ReachabilityGraph<'tcx> {
        // ExpnId::root() contains CRATE_DEF_ID as its macro_def_id, but that isn't used for
        // anything.
        // FIXME add newtype that ensures it really isn't used
        let root_expansion = self.nodes.get_index_of(&Node::from(LocalExpnId::ROOT)).unwrap();
        let crate_root = self.nodes.get_index_of(&Node::from(CRATE_DEF_ID)).unwrap();
        self.edges[root_expansion].remove(&crate_root);

        // Node identity hashing can trigger compiler queries, so discard
        // unreachable metadata before hashing nodes or constructing SCCs.
        // Traversal order cannot affect membership in the reachable set, and
        // the retained nodes and edges are stably sorted below.
        #[allow(rustc::potential_query_instability)]
        let reachable = {
            let mut reachable = IndexVec::from_elem_n(false, self.nodes.len());
            let mut worklist: Vec<_> = self.roots.union(&self.retained).copied().collect();
            for &root in &worklist {
                reachable[root] = true;
            }
            while let Some(node) = worklist.pop() {
                worklist.extend(
                    self.edges[node]
                        .iter()
                        .copied()
                        .filter(|&edge| !std::mem::replace(&mut reachable[edge], true)),
                );
            }
            reachable
        };

        let mut order: Vec<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(index, _)| reachable[*index])
            .map(|(index, node)| (stable_hash(hcx, node), index))
            .collect();
        // Break ties by insertion order, as in a stable sort by fingerprint.
        order.sort_unstable();

        let mut node_indices = IndexVec::from_elem_n(usize::MAX, self.nodes.len());
        let nodes: Vec<_> = order
            .into_iter()
            .enumerate()
            .map(|(index, (hash, original))| {
                node_indices[original] = index;
                (self.nodes[original], hash)
            })
            .collect();
        drop(self.nodes);

        // iterating here is fine, as we stable sort right after
        #[allow(rustc::potential_query_instability)]
        let mut roots: Vec<_> = self.roots.into_iter().map(|node| node_indices[node]).collect();
        roots.sort();
        let mut edges = IndexVec::from_elem_n(Vec::default(), nodes.len());
        for (node, node_edges) in self.edges.into_iter_enumerated() {
            if !reachable[node] {
                continue;
            }
            // iterating here is fine, as we stable sort right after
            #[allow(rustc::potential_query_instability)]
            let mut node_edges: Vec<_> =
                node_edges.into_iter().map(|node| node_indices[node]).collect();
            node_edges.sort();
            edges[node_indices[node]] = node_edges;
        }

        ReachabilityGraph { roots, edges, nodes }
    }

    /// Adds an edge from the active [`RecordMode::From`] node, suppresses the node under
    /// [`RecordMode::None`], or records it as a graph root when no mode is active.
    pub(super) fn record(&mut self, to: Node<'tcx>) {
        match self.record_mode.last().copied() {
            Some(Some(from)) => {
                let to = self.node_index(to);
                self.edges[from].insert(to);
            }
            Some(None) => (),
            None => {
                let to = self.node_index(to);
                self.roots.insert(to);
            }
        }
    }
}

/// Accumulates the hashes of encoded payloads associated with each semantic graph node.
#[derive(Default)]
pub(crate) struct ReachabilityGraphHashes {
    local: UnordMap<LocalDefId, Fingerprint>,
    non_local: UnordMap<DefId, Fingerprint>,
    expn: UnordMap<LocalExpnId, Fingerprint>,
    syntax: UnordMap<SyntaxContext, Fingerprint>,
}

impl ReachabilityGraphHashes {
    fn add_node_hash(&mut self, node: &Node<'_>, hash: Fingerprint) {
        let current_hash = match node {
            Node::DefId(id) => {
                if let Some(local) = id.as_local() {
                    self.local.entry(local).or_insert(Fingerprint::ZERO)
                } else {
                    self.non_local.entry(*id).or_insert(Fingerprint::ZERO)
                }
            }
            Node::ExpnId(id) => self.expn.entry(id.expect_local()).or_insert(Fingerprint::ZERO),
            Node::SyntaxContext(id) => self.syntax.entry(*id).or_insert(Fingerprint::ZERO),
            Node::Span(_) => unimplemented!(),
            Node::Ty(_) => unimplemented!(),
            Node::Predicate(_) => unimplemented!(),
            Node::AllocId(_) | Node::MetadataLocation(_) => unimplemented!(),
        };
        *current_hash = current_hash.combine_commutative(hash);
    }

    fn get_node(&self, node: &Node<'_>, tcx: TyCtxt<'_>) -> Option<Fingerprint> {
        match node {
            Node::DefId(id) => {
                if let Some(local) = id.as_local() {
                    self.local.get(&local).copied()
                } else {
                    let extern_hash = tcx.extern_def_public_hash(*id);
                    Some(if let Some(local_hash) = self.non_local.get(id) {
                        local_hash.combine_commutative(extern_hash)
                    } else {
                        extern_hash
                    })
                }
            }
            Node::ExpnId(id) => {
                if let Some(local) = id.as_local() {
                    self.expn.get(&local).copied()
                } else {
                    Some(tcx.extern_expn_public_hash(*id))
                }
            }
            Node::SyntaxContext(id) => self.syntax.get(id).copied(),
            Node::Span(span) => tcx.with_stable_hashing_context(|hcx| {
                let hcx = RefCell::new(hcx);
                tcx.sess
                    .source_map()
                    .span_to_source(*span, |source, start, end| {
                        // Proc-macro subspans can split a UTF-8 character.
                        Ok(stable_hash(&mut hcx.borrow_mut(), &source.as_bytes()[start..end]))
                    })
                    .ok()
            }),
            Node::Ty(_) => None,
            Node::Predicate(_) | Node::MetadataLocation(_) => None,
            Node::AllocId(id) => Some(tcx.with_stable_hashing_context(|mut hcx| {
                stable_hash(&mut hcx, &tcx.global_alloc(*id))
            })),
        }
    }
}

/// A semantic identity whose metadata dependencies can participate in the public API graph.
#[derive(PartialEq, Eq, Clone, Copy, Hash, StableHash, Debug)]
pub(crate) enum Node<'tcx> {
    DefId(DefId),
    ExpnId(ExpnId),
    SyntaxContext(SyntaxContext),
    Span(Span),
    Ty(Ty<'tcx>),
    Predicate(PredicateKind<'tcx>),
    AllocId(AllocId),
    MetadataLocation(Fingerprint),
}

impl From<AllocId> for Node<'_> {
    #[inline(always)]
    fn from(value: AllocId) -> Self {
        Self::AllocId(value)
    }
}
impl From<Span> for Node<'_> {
    #[inline(always)]
    fn from(value: Span) -> Self {
        Self::Span(value)
    }
}
impl<'tcx> From<Ty<'tcx>> for Node<'tcx> {
    #[inline(always)]
    fn from(value: Ty<'tcx>) -> Self {
        Self::Ty(value)
    }
}
impl<'tcx> From<PredicateKind<'tcx>> for Node<'tcx> {
    #[inline(always)]
    fn from(value: PredicateKind<'tcx>) -> Self {
        Self::Predicate(value)
    }
}
impl From<LocalDefId> for Node<'_> {
    #[inline(always)]
    fn from(value: LocalDefId) -> Self {
        Self::DefId(value.into())
    }
}
impl From<DefId> for Node<'_> {
    #[inline(always)]
    fn from(value: DefId) -> Self {
        Self::DefId(value)
    }
}
impl From<LocalExpnId> for Node<'_> {
    #[inline(always)]
    fn from(value: LocalExpnId) -> Self {
        Self::ExpnId(value.to_expn_id())
    }
}
impl From<DefIndex> for Node<'_> {
    #[inline(always)]
    fn from(local_def_index: DefIndex) -> Self {
        LocalDefId { local_def_index }.into()
    }
}
impl From<ExpnId> for Node<'_> {
    #[inline(always)]
    fn from(value: ExpnId) -> Self {
        Self::ExpnId(value)
    }
}
impl From<SyntaxContext> for Node<'_> {
    #[inline(always)]
    fn from(value: SyntaxContext) -> Self {
        Self::SyntaxContext(value)
    }
}

/// Converts a semantic table key into both its on-disk index and reachability-graph node.
pub(crate) trait TableIndex: Copy + for<'a> Into<Node<'a>> {
    type Encoded: Idx;
    fn into_encoded(self) -> Self::Encoded;
}

impl TableIndex for DefIndex {
    type Encoded = DefIndex;
    fn into_encoded(self) -> Self::Encoded {
        self
    }
}

impl TableIndex for LocalDefId {
    type Encoded = DefIndex;
    fn into_encoded(self) -> Self::Encoded {
        self.local_def_index
    }
}

impl TableIndex for LocalExpnId {
    type Encoded = ExpnIndex;
    fn into_encoded(self) -> Self::Encoded {
        self.as_raw()
    }
}

impl TableIndex for SyntaxContext {
    type Encoded = u32;
    fn into_encoded(self) -> Self::Encoded {
        self.as_u32()
    }
}

impl DirectedGraph for ReachabilityGraph<'_> {
    type Node = NodeIdx;

    fn num_nodes(&self) -> usize {
        self.nodes.len()
    }
}

impl Successors for ReachabilityGraph<'_> {
    fn successors(&self, node: Self::Node) -> impl Iterator<Item = Self::Node> {
        self.edges[node].iter().copied()
    }
}

type NodeIdx = usize;
type SccIdx = u32;

/// The fingerprint of a single node: its identity hash combined with the hash
/// of the data encoded for it (if any).
fn node_fingerprint(
    graph: &ReachabilityGraph<'_>,
    hashes: &ReachabilityGraphHashes,
    tcx: TyCtxt<'_>,
    node_index: NodeIdx,
) -> Fingerprint {
    let (node_key, base_hash) = &graph.nodes[node_index];
    match hashes.get_node(node_key, tcx) {
        Some(encoded_data_hash) => base_hash.combine_commutative(encoded_data_hash),
        None => *base_hash,
    }
}

/// Aggregate the per-node fingerprints into one fingerprint per SCC that
/// captures everything the SCC (transitively) reaches.
///
/// We deliberately do *not* use [`Sccs::new_with_annotation`] for this. That
/// machinery propagates annotations along the DFS as it discovers cycles, which
/// double-counts the cycle's entry node (and re-counts SCCs reached along
/// multiple paths). With an idempotent merge (`min`/`max`/set-union, as in
/// borrowck) that's invisible, but our merge is `Fingerprint::combine_commutative`
/// (128-bit wrapping addition), which is *not* idempotent, so the result would
/// depend on the DFS entry order — and therefore on unreachable nodes, which act
/// as extra DFS roots.
///
/// Instead we compute the fingerprints purely from the (DFS-order-independent)
/// condensation topology:
///   * `base(scc)` = sum of its member nodes' fingerprints, each counted once;
///   * `fingerprint(scc)` = `base(scc)` plus the fingerprint of every successor
///     SCC.
///
/// [`Sccs::all_sccs`] yields the condensation in dependency order (successors
/// before predecessors), so a single pass sees each successor's fingerprint
/// already finished.
fn scc_fingerprints(
    graph: &ReachabilityGraph<'_>,
    hashes: &ReachabilityGraphHashes,
    tcx: TyCtxt<'_>,
    sccs: &Sccs<NodeIdx, SccIdx>,
) -> IndexVec<SccIdx, Fingerprint> {
    let mut fingerprints: IndexVec<SccIdx, Fingerprint> =
        IndexVec::from_elem_n(Fingerprint::ZERO, sccs.num_sccs());
    for node_index in 0..graph.nodes.len() {
        let scc = sccs.scc(node_index);
        fingerprints[scc] =
            fingerprints[scc].combine_commutative(node_fingerprint(graph, hashes, tcx, node_index));
    }
    for scc in sccs.all_sccs() {
        for &succ in sccs.successors(scc) {
            fingerprints[scc] = fingerprints[scc].combine_commutative(fingerprints[succ]);
        }
    }
    fingerprints
}

/// Computes transitive SCC fingerprints and persists them for reachable local definitions and
/// expansions, together with the combined hash of the graph roots.
fn build_public_hashes<'tcx>(
    graph: &ReachabilityGraph<'tcx>,
    hashes: &ReachabilityGraphHashes,
    tcx: TyCtxt<'tcx>,
    hcx: &mut StableHashState<'_, true>,
) -> Hashed<ItemPublicHashesBuilder> {
    let sccs = Sccs::<_, SccIdx>::new(graph);
    let scc_fingerprints = scc_fingerprints(graph, hashes, tcx, &sccs);

    let mut public_hashes = ItemPublicHashesBuilder::default();
    let mut hasher = StableHasher::new();
    for (node_index, (node, _)) in graph.nodes.iter().enumerate() {
        match node {
            Node::DefId(id) => {
                if let Some(local) = id.as_local() {
                    let fingerprint = scc_fingerprints[sccs.scc(node_index)];
                    public_hashes.local.set_some_unhashed(local.local_def_index, fingerprint);
                }
            }
            Node::ExpnId(id) => {
                if let Some(local) = id.as_local() {
                    let fingerprint = scc_fingerprints[sccs.scc(node_index)];
                    public_hashes.expn.set_some_unhashed(local.as_raw(), fingerprint);
                }
            }
            Node::SyntaxContext(_) => (),
            Node::Ty(_) => (),
            Node::Predicate(_) => (),
            Node::Span(_) => (),
            Node::AllocId(_) | Node::MetadataLocation(_) => (),
        }
    }
    for &node_index in &graph.roots {
        let fingerprint = scc_fingerprints[sccs.scc(node_index)];
        fingerprint.stable_hash(hcx, &mut hasher);
    }
    let roots_hash = hasher.finish::<Fingerprint>();
    debug!("build_public_hashes: roots fingerprint hash = {roots_hash:?}");
    Hashed { value: public_hashes, hash: Some(roots_hash) }
}

#[derive(Default)]
pub(crate) struct ItemPublicHashesBuilder {
    local: TableBuilder<RDRHashNone<DefIndex>, DefIndex, Option<Fingerprint>>,
    expn: TableBuilder<RDRHashNone<ExpnIndex>, ExpnIndex, Option<Fingerprint>>,
}

impl ItemPublicHashesBuilder {
    fn encode<'a, 'tcx, M: MetadataEncoder<'tcx>>(
        &self,
        ecx: &mut EncodeContext<'a, 'tcx, M>,
        public_global_hash: Fingerprint,
    ) -> ItemPublicHashes {
        ItemPublicHashes {
            local: self.local.encode(ecx).0,
            expn: self.expn.encode(ecx).0,
            public_global_hash,
        }
    }
}

/// Per-item public hashes persisted in rmeta, plus the graph-independent global hash.
#[derive(LazyDecodable, MetadataEncodable)]
pub(crate) struct ItemPublicHashes {
    pub(crate) local: LazyTable<DefIndex, Option<Fingerprint>>,
    pub(crate) expn: LazyTable<ExpnIndex, Option<Fingerprint>>,
    pub(crate) public_global_hash: Fingerprint,
}
