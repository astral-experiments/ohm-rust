use rustc_data_structures::fx::FxIndexSet;
use rustc_data_structures::unord::UnordSet;
use rustc_errors::ErrorGuaranteed;
use rustc_hir::HirId;
use rustc_hir::def_id::{DefId, LocalDefId, LocalDefIdMap, LocalDefIdSet};
use rustc_macros::{StableHash, TyDecodable, TyEncodable};

/// The names that one typed body uses, independently of which bodies are live.
#[derive(Clone, Copy, Debug, StableHash, TyDecodable, TyEncodable)]
pub struct BodyUses<'tcx> {
    pub trait_imports: &'tcx UnordSet<LocalDefId>,
    pub tainted_by_errors: Option<ErrorGuaranteed>,
    pub symbols: &'tcx [SymbolUse],
}

/// An effect of visiting a live definition during dead-code analysis.
#[derive(Clone, Copy, Debug, StableHash, TyDecodable, TyEncodable)]
pub enum SymbolUse {
    /// Schedule a referenced definition under the current lint context.
    Explore(LocalDefId),
    /// Mark a leaf entry (such as a field) under the current lint context.
    Mark(LocalDefId),
    /// Inline constants and visible fields are always live.
    MarkUnconditionally(LocalDefId),
    /// Start a fresh traversal for an associated constant in a trait constraint.
    ExploreUnconditionally(LocalDefId),
    /// Diagnose self-assignment when the containing body becomes live.
    SelfAssignment(HirId),
}

/// A single snapshot of dead-code liveness analysis state.
#[derive(Clone, Debug, StableHash)]
pub struct DeadCodeLivenessSnapshot {
    pub live_symbols: LocalDefIdSet,
    /// Maps each ADT to derived traits (for example `Debug` and `Clone`) that should be ignored
    /// when checking for dead code diagnostics.
    pub ignored_derived_traits: LocalDefIdMap<FxIndexSet<DefId>>,
}

/// Dead-code liveness data for both analysis phases.
///
/// `pre_deferred_seeding` is computed before reachable-public and `#[allow(dead_code)]` seeding,
/// and is used for lint `dead_code_pub_in_binary`.
/// `final_result` is the final liveness snapshot used for lint `dead_code`.
#[derive(Clone, Debug, StableHash)]
pub struct DeadCodeLivenessSummary {
    pub pre_deferred_seeding: DeadCodeLivenessSnapshot,
    pub final_result: DeadCodeLivenessSnapshot,
}
