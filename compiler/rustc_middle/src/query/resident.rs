use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, ThreadId};

use rustc_data_structures::fx::{FxHashMap, FxHashSet, FxIndexSet};
use rustc_data_structures::sync::Lock;
use rustc_errors::{DiagInner, MultiSpan};
use rustc_hir::HirId;
use rustc_lint_defs::LintId;
use rustc_span::Symbol;
use rustc_span::def_id::{DefId, DefIdSet, LocalDefId};

use super::QueryJobId;
use super::resident_graph::Graph;
use crate::queries::TaggedQueryKey;
use crate::ty::{self, TyCtxt, tls};

/// Current-process query dependencies for the experimental, non-disk resident checker.
///
/// Reads are recorded even when they hit rustc's ordinary in-memory query caches. Completed
/// providers replace their previous dependencies; query feeding contributes a conservative
/// fed-query-to-feeder edge. This deliberately does not change rustc's disk dep graph or evict
/// query values. Consumers must apply a returned invalidation plan to caches at quiescence.
pub struct ResidentQueryGraph<'tcx> {
    graph: Lock<Graph<ResidentQueryKey<'tcx>, QueryJobId>>,
    trait_tasks: Lock<FxHashMap<ThreadId, Vec<TraitTask<'tcx>>>>,
    delayed_attribute_lints: Lock<Option<Vec<ResidentDelayedAttributeLint>>>,
    codegen_cache: Lock<Option<Arc<ResidentCodegenCache>>>,
}

impl Default for ResidentQueryGraph<'_> {
    fn default() -> Self {
        Self {
            graph: Lock::new(Graph::default()),
            trait_tasks: Default::default(),
            delayed_attribute_lints: Default::default(),
            codegen_cache: Default::default(),
        }
    }
}

/// Private artifacts retained after a successful LLVM build and before the linker may remove them.
#[derive(Clone)]
pub struct ResidentCodegenArtifacts {
    pub object: PathBuf,
    pub global_asm_object: Option<PathBuf>,
    pub dwarf_object: Option<PathBuf>,
    pub fingerprint: String,
}

#[derive(Clone)]
pub struct ResidentCodegenOutcome {
    pub name: String,
    pub reused: bool,
    pub fingerprint: String,
}

struct ResidentCodegenEntry {
    partition: String,
    clean: bool,
    artifacts: ResidentCodegenArtifacts,
    membership_answers: Vec<(DefId, bool)>,
}

struct ResidentPendingCodegen {
    partition: String,
    reused: bool,
    membership: Option<FxIndexSet<DefId>>,
}

#[derive(Default)]
struct ResidentCodegenState {
    entries: FxHashMap<String, ResidentCodegenEntry>,
    pending: FxHashMap<String, ResidentPendingCodegen>,
    membership: DefIdSet,
    report: Vec<ResidentCodegenOutcome>,
}

/// An explicitly enabled, session-private object cache. Query dependencies, not object equality,
/// decide whether an artifact is reusable; the fingerprint is exposed as supporting evidence.
pub struct ResidentCodegenCache {
    directory: PathBuf,
    state: Lock<ResidentCodegenState>,
}

impl ResidentCodegenCache {
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn begin(&self, membership: &DefIdSet) {
        let mut state = self.state.lock();
        state.pending.clear();
        state.report.clear();
        state.membership = membership.clone();
    }

    pub fn lookup(&self, name: &str, partition: String) -> Option<ResidentCodegenArtifacts> {
        let mut state = self.state.lock();
        let artifact = state.entries.get(name).and_then(|entry| {
            (entry.clean
                && entry.partition == partition
                && entry
                    .membership_answers
                    .iter()
                    .all(|(def_id, answer)| state.membership.contains(def_id) == *answer)
                && entry.artifacts.object.is_file()
                && entry.artifacts.global_asm_object.as_deref().is_none_or(Path::is_file)
                && entry.artifacts.dwarf_object.as_deref().is_none_or(Path::is_file))
            .then(|| entry.artifacts.clone())
        });
        state.pending.insert(
            name.to_owned(),
            ResidentPendingCodegen { partition, reused: artifact.is_some(), membership: None },
        );
        artifact
    }

    fn record_membership(&self, name: Symbol, membership: FxIndexSet<DefId>) {
        if let Some(pending) = self.state.lock().pending.get_mut(name.as_str()) {
            pending.membership = Some(membership);
        }
    }

    pub fn publish(&self, name: &str, artifacts: ResidentCodegenArtifacts) {
        let mut state = self.state.lock();
        let Some(ResidentPendingCodegen { partition, reused, membership }) =
            state.pending.remove(name)
        else {
            return;
        };
        state.report.push(ResidentCodegenOutcome {
            name: name.to_owned(),
            reused,
            fingerprint: artifacts.fingerprint.clone(),
        });
        let membership_answers = if reused {
            state.entries.get(name).unwrap().membership_answers.clone()
        } else if let Some(membership) = membership {
            membership
                .into_iter()
                .map(|def_id| (def_id, state.membership.contains(&def_id)))
                .collect()
        } else {
            // The backend did not report complete actual dependencies; never reuse its output.
            state.entries.remove(name);
            return;
        };
        state.entries.insert(
            name.to_owned(),
            ResidentCodegenEntry { partition, clean: true, artifacts, membership_answers },
        );
    }

    pub fn reused(&self, name: &str) -> Option<ResidentCodegenArtifacts> {
        let state = self.state.lock();
        state
            .pending
            .get(name)
            .filter(|pending| pending.reused)
            .and_then(|_| state.entries.get(name).map(|entry| entry.artifacts.clone()))
    }

    fn invalidate(&self, name: Symbol) {
        if let Some(entry) = self.state.lock().entries.get_mut(name.as_str()) {
            entry.clean = false;
        }
    }

    fn report(&self) -> Vec<ResidentCodegenOutcome> {
        let mut report = self.state.lock().report.clone();
        report.sort_by(|left, right| left.name.cmp(&right.name));
        report
    }
}

/// A delayed AST-attribute lint whose one-shot diagnostic decorator has already been evaluated.
/// Supported resident body edits cannot change the attributes that originally produced it.
#[derive(Clone)]
pub struct ResidentDelayedAttributeLint {
    pub lint: LintId,
    pub hir_id: HirId,
    pub span: MultiSpan,
    pub diagnostic: DiagInner,
}

struct TraitTask<'tcx> {
    key: ResidentQueryKey<'tcx>,
    reads: FxHashSet<ResidentQueryKey<'tcx>>,
    codegen_membership: FxIndexSet<DefId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResidentOldTraitKey<'tcx> {
    Selection(ty::TypingEnv<'tcx>, ty::TraitPredicate<'tcx>),
    Evaluation(ty::TypingEnv<'tcx>, ty::PolyTraitPredicate<'tcx>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResidentQueryKey<'tcx> {
    Query(TaggedQueryKey<'tcx>),
    /// Bodies and interior HIR nodes belonging to this owner, independently of its signature.
    HirBody(LocalDefId),
    /// Structural body properties: item/attribute topology and lifetime/generic binders.
    /// Current literal and simple-local edits preserve this independent input.
    HirStructure(LocalDefId),
    /// A keyed entry in the legacy trait solver's global in-memory caches.
    OldTraitSolver(ResidentOldTraitKey<'tcx>),
    /// The next trait solver's global, otherwise untracked, in-memory cache.
    TraitSolver,
    /// Real rustc queries and HIR inputs accessed while lowering one LLVM module.
    CodegenUnit(Symbol),
}

impl<'tcx> From<TaggedQueryKey<'tcx>> for ResidentQueryKey<'tcx> {
    fn from(value: TaggedQueryKey<'tcx>) -> Self {
        Self::Query(value)
    }
}

impl ResidentQueryKey<'_> {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Query(query) => query.query_name(),
            Self::HirBody(_) => "resident_hir_body",
            Self::HirStructure(_) => "resident_hir_structure",
            Self::OldTraitSolver(ResidentOldTraitKey::Selection(..)) => "resident_trait_selection",
            Self::OldTraitSolver(ResidentOldTraitKey::Evaluation(..)) => {
                "resident_trait_evaluation"
            }
            Self::TraitSolver => "resident_trait_solver",
            Self::CodegenUnit(_) => "resident_codegen_unit",
        }
    }

    pub fn def_id(&self) -> Option<DefId> {
        match self {
            Self::Query(query) => query.resident_key_def_id(),
            Self::HirBody(def_id) | Self::HirStructure(def_id) => Some(def_id.to_def_id()),
            Self::OldTraitSolver(_) | Self::TraitSolver | Self::CodegenUnit(_) => None,
        }
    }
}

pub struct ResidentInvalidation<'tcx> {
    pub revision: u64,
    /// Includes each requested input and all currently recorded transitive readers.
    /// Ordering is unspecified.
    pub queries: Vec<ResidentQueryKey<'tcx>>,
}

/// The result of applying a resident revision to rustc's real in-memory caches.
pub struct ResidentEviction<'tcx> {
    pub revision: u64,
    /// Changed non-query inputs, which do not have an entry in a rustc query cache.
    pub inputs: Vec<LocalDefId>,
    /// Changed structural HIR inputs, which are independent of ordinary body values.
    pub structural_inputs: Vec<LocalDefId>,
    /// Query keys that had a completed value, which is now evicted.
    pub removed: Vec<TaggedQueryKey<'tcx>>,
    /// Recorded keys without a cached value when the plan was applied.
    pub uncached: Vec<TaggedQueryKey<'tcx>>,
    /// True when non-query trait caches were reset and their recorded readers were invalidated.
    pub trait_solver_caches_reset: bool,
}

pub struct ResidentGraphStats {
    pub revision: u64,
    pub completed_providers: usize,
    pub edges: usize,
}

// Invalidation is a set: the public inspection API explicitly gives no traversal order, and no
// ordered data from this experimental sidecar contributes to a rustc query result.
#[expect(rustc::potential_query_instability)]
impl<'tcx> ResidentQueryGraph<'tcx> {
    /// Opts a single-threaded embedding driver into process-local object reuse. The directory must
    /// already exist and must be private to this compiler session.
    pub fn configure_codegen_cache(&self, directory: PathBuf) -> Result<(), &'static str> {
        if !directory.is_dir() {
            return Err("resident codegen cache directory does not exist");
        }
        let mut cache = self.codegen_cache.lock();
        if cache.is_some() {
            return Err("resident codegen cache already configured");
        }
        *cache = Some(Arc::new(ResidentCodegenCache { directory, state: Default::default() }));
        Ok(())
    }

    pub fn codegen_cache(&self) -> Option<Arc<ResidentCodegenCache>> {
        self.codegen_cache.lock().clone()
    }

    /// Built/reused CGUs and the fingerprints of their actual regular object files, in name order.
    pub fn codegen_cache_report(&self) -> Vec<ResidentCodegenOutcome> {
        self.codegen_cache().map_or_else(Vec::new, |cache| cache.report())
    }

    /// Whether an embedding driver has started a new resident revision after the original crate.
    /// Unlike graph inspection, this can be read by the running crate analysis provider.
    pub fn is_rechecking(&self) -> bool {
        self.graph.lock().revision() != 0
    }

    /// Retrieve templates for replaying immutable AST-attribute lints in a later full analysis.
    pub fn delayed_attribute_lints(&self) -> Option<Vec<ResidentDelayedAttributeLint>> {
        self.delayed_attribute_lints.lock().clone()
    }

    /// Retain the templates after the crate's original one-shot HIR lint buffers were drained.
    pub fn store_delayed_attribute_lints(&self, lints: Vec<ResidentDelayedAttributeLint>) {
        assert!(self.delayed_attribute_lints.lock().replace(lints).is_none());
    }

    #[inline]
    pub(crate) fn read(&self, key: TaggedQueryKey<'tcx>, accesses_value: bool) {
        self.read_trait_task(key.into());
        if let Some(parent) = tls::with_context(|icx| icx.query) {
            let mut graph = self.graph.lock();
            graph.read(parent, key.into());
            // A previous reader can consume these body-level `Steal` values. If any other reader
            // is invalidated via an independent input, it cannot safely reuse the cached producer:
            // its value may already be gone. Restore the producer (and therefore its other readers)
            // whenever a recorded value reader is invalidated. `ensure_ok` and `ensure_result`
            // discard the value, so they can safely reuse a previously consumed producer and must
            // not force it merely because the orchestrating query is rerunning. The crate-level
            // AST `Steal` queries are outside the supported body-edit boundary.
            if accesses_value
                && matches!(
                    key,
                    TaggedQueryKey::thir_body(_)
                        | TaggedQueryKey::mir_built(_)
                        | TaggedQueryKey::mir_promoted(_)
                        | TaggedQueryKey::mir_drops_elaborated_and_const_checked(_)
                        | TaggedQueryKey::mir_coroutine_layout_prepared(_)
                )
            {
                graph.feed(parent, key.into());
            }
        }
    }

    /// Mark access to actual body/interior HIR, separately from the containing owner's signature.
    #[inline]
    pub(crate) fn read_hir_body(&self, owner: LocalDefId) {
        let key = if let Some(parent) = tls::with_context(|icx| icx.query) {
            let mut graph = self.graph.lock();
            let key = match graph.running_key(parent) {
                // These providers traverse bodies to collect structural information. The editing
                // API only changes literals/simple locals and rejects macros, nested items,
                // closures, anonymous constants, generics and added type/lifetime binders.
                // `fn_arg_idents` inspects only the body's unchanged parameter names and spans.
                // A future edit that can alter any of these must invalidate HirStructure as well.
                Some(ResidentQueryKey::Query(
                    TaggedQueryKey::hir_crate_items(_)
                    | TaggedQueryKey::resolve_bound_vars(_)
                    | TaggedQueryKey::fn_arg_idents(_),
                )) => ResidentQueryKey::HirStructure(owner),
                _ => ResidentQueryKey::HirBody(owner),
            };
            graph.read(parent, key);
            key
        } else {
            ResidentQueryKey::HirBody(owner)
        };
        self.read_trait_task(key);
    }

    /// Mark access to solver-global caches whose entries are not represented as rustc queries.
    #[inline]
    pub(crate) fn read_trait_solver(&self) {
        self.read_trait_task(ResidentQueryKey::TraitSolver);
        if let Some(parent) = tls::with_context(|icx| icx.query) {
            self.graph.lock().read(parent, ResidentQueryKey::TraitSolver);
        }
    }

    fn read_trait_task(&self, key: ResidentQueryKey<'tcx>) {
        if let Some(task) =
            self.trait_tasks.lock().get_mut(&thread::current().id()).and_then(|s| s.last_mut())
            && key != task.key
        {
            if matches!(task.key, ResidentQueryKey::CodegenUnit(_)) {
                match key {
                    // Each build independently compares its full current CGU partition (membership,
                    // linkage, visibility), instead of the broad crate-wide collector dependency.
                    ResidentQueryKey::Query(
                        TaggedQueryKey::codegen_unit(_)
                        | TaggedQueryKey::collect_and_partition_mono_items(_),
                    ) => return,
                    // The provider is exactly membership in the independently recomputed mono set.
                    // Retain only the keys this CGU actually observed and check their current answers.
                    ResidentQueryKey::Query(TaggedQueryKey::is_codegened_item(def_id)) => {
                        task.codegen_membership.insert(def_id);
                        return;
                    }
                    _ => {}
                }
            }
            task.reads.insert(key);
        }
    }

    /// Record a global legacy solver lookup even when it hits an existing answer.
    pub fn read_old_trait_solver(&self, key: ResidentOldTraitKey<'tcx>) {
        let key = ResidentQueryKey::OldTraitSolver(key);
        self.read_trait_task(key);
        if let Some(parent) = tls::with_context(|icx| icx.query) {
            self.graph.lock().read(parent, key);
        }
    }

    /// Capture real query reads and nested global solver lookups on a legacy cache miss.
    pub fn with_old_trait_solver<R>(
        &self,
        key: ResidentOldTraitKey<'tcx>,
        compute: impl FnOnce() -> R,
    ) -> R {
        let key = ResidentQueryKey::OldTraitSolver(key);
        self.with_external(key, compute)
    }

    /// Record all real query/HIR dependencies accessed while LLVM lowers a CGU.
    pub fn with_codegen_unit<R>(&self, name: Symbol, compute: impl FnOnce() -> R) -> R {
        if self.codegen_cache.lock().is_none() {
            return compute();
        }
        self.with_external(ResidentQueryKey::CodegenUnit(name), compute)
    }

    fn with_external<R>(&self, key: ResidentQueryKey<'tcx>, compute: impl FnOnce() -> R) -> R {
        let thread = thread::current().id();
        self.trait_tasks.lock().entry(thread).or_default().push(TraitTask {
            key,
            reads: FxHashSet::default(),
            codegen_membership: FxIndexSet::default(),
        });
        let finished = Cell::new(false);
        let _end = rustc_data_structures::defer(|| {
            let task = self.trait_tasks.lock().get_mut(&thread).unwrap().pop().unwrap();
            if finished.get() {
                self.graph.lock().record_external(task.key, task.reads);
                if let ResidentQueryKey::CodegenUnit(name) = task.key
                    && let Some(cache) = self.codegen_cache()
                {
                    cache.record_membership(name, task.codegen_membership);
                }
            }
        });
        let result = compute();
        finished.set(true);
        result
    }

    #[inline]
    pub(crate) fn feed(&self, key: TaggedQueryKey<'tcx>) {
        if let Some(parent) = tls::with_context(|icx| icx.query) {
            self.graph.lock().feed(parent, key.into());
        }
    }

    /// Run a real query provider, replacing its previous read set only if it returns normally.
    #[inline]
    pub fn with_provider<R>(
        &self,
        job: QueryJobId,
        key: TaggedQueryKey<'tcx>,
        provider: impl FnOnce() -> R,
    ) -> R {
        self.graph.lock().begin(job, key.into());
        let finished = Cell::new(false);
        let _end = rustc_data_structures::defer(|| self.graph.lock().end(job, finished.get()));
        let result = provider();
        finished.set(true);
        result
    }

    /// Plan a new revision from changed query inputs. No query provider may still be running.
    /// This plans eager invalidation; it does not implement Salsa's unchanged-value backdating.
    pub fn advance(
        &self,
        inputs: impl IntoIterator<Item = ResidentQueryKey<'tcx>>,
    ) -> ResidentInvalidation<'tcx> {
        let (revision, queries) = self.graph.lock().advance(inputs);
        ResidentInvalidation { revision, queries: queries.into_iter().collect() }
    }

    /// Inspect all currently recorded transitive readers without advancing the revision.
    pub fn affected(
        &self,
        inputs: impl IntoIterator<Item = ResidentQueryKey<'tcx>>,
    ) -> Vec<ResidentQueryKey<'tcx>> {
        self.graph.lock().closure(inputs).into_iter().collect()
    }

    /// Inspect the direct query inputs most recently recorded for this query, if any.
    pub fn dependencies(&self, key: ResidentQueryKey<'tcx>) -> Vec<ResidentQueryKey<'tcx>> {
        self.graph.lock().dependencies(key).into_iter().collect()
    }

    pub fn stats(&self) -> ResidentGraphStats {
        let (revision, completed_providers, edges) = self.graph.lock().stats();
        ResidentGraphStats { revision, completed_providers, edges }
    }
}

impl<'tcx> TyCtxt<'tcx> {
    /// Advances the live revision and removes all currently known readers of the changed inputs.
    ///
    /// This never runs providers. An embedding driver must re-request the diagnostic roots it
    /// wants to report, and must reset/isolate diagnostics at each revision. Query values already
    /// handed to the driver still refer to the old revision and must no longer be consulted.
    ///
    /// # Safety
    /// The embedding driver must own the only compiler thread, with all previous query/cache
    /// accesses completed and no access started until this call returns.
    pub unsafe fn ohm_invalidate_resident_queries(
        self,
        inputs: impl IntoIterator<Item = ResidentQueryKey<'tcx>>,
    ) -> Result<ResidentEviction<'tcx>, &'static str> {
        if self.dep_graph.is_fully_enabled() || self.sess.threads().is_some() {
            return Err(
                "requires a single-threaded compiler with incremental compilation disabled",
            );
        }
        if tls::with_context(|context| context.query.is_some()) {
            return Err("cannot invalidate from inside an active query");
        }
        let graph =
            self.query_system.resident.as_ref().ok_or("resident query graph is disabled")?;
        let mut inputs: Vec<_> = inputs.into_iter().collect();
        // The next trait solver does not yet provide individually keyed resident dependencies.
        // Legacy solver answers are reached through the real queries and nested solver goals that
        // produced them, including when a later query uses an already cached solver answer.
        if graph.affected(inputs.iter().copied()).iter().any(|key| {
            matches!(
                key,
                ResidentQueryKey::Query(
                    TaggedQueryKey::type_of_opaque(_)
                        | TaggedQueryKey::type_of_opaque_hir_typeck(_)
                )
            )
        }) {
            inputs.push(ResidentQueryKey::TraitSolver);
        }
        let plan = graph.advance(inputs);
        let mut eviction = ResidentEviction {
            revision: plan.revision,
            inputs: Vec::new(),
            structural_inputs: Vec::new(),
            removed: Vec::new(),
            uncached: Vec::new(),
            trait_solver_caches_reset: false,
        };
        for key in plan.queries {
            match key {
                ResidentQueryKey::HirBody(owner) => eviction.inputs.push(owner),
                ResidentQueryKey::HirStructure(owner) => eviction.structural_inputs.push(owner),
                ResidentQueryKey::OldTraitSolver(ResidentOldTraitKey::Selection(env, pred)) => {
                    eviction.trait_solver_caches_reset |=
                        self.selection_cache.remove_resident(&(env, pred));
                }
                ResidentQueryKey::OldTraitSolver(ResidentOldTraitKey::Evaluation(env, pred)) => {
                    eviction.trait_solver_caches_reset |=
                        self.evaluation_cache.remove_resident(&(env, pred));
                }
                ResidentQueryKey::TraitSolver => {
                    *self.new_solver_evaluation_cache.lock() = Default::default();
                    eviction.trait_solver_caches_reset = true;
                }
                ResidentQueryKey::CodegenUnit(name) => {
                    if let Some(cache) = graph.codegen_cache() {
                        cache.invalidate(name);
                    }
                }
                ResidentQueryKey::Query(query) => {
                    // SAFETY: The caller owns the only compiler thread between query requests.
                    if unsafe { query.resident_invalidate_quiescent(self) } {
                        eviction.removed.push(query);
                    } else {
                        eviction.uncached.push(query);
                    }
                }
            }
        }
        Ok(eviction)
    }
}
