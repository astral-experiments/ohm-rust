use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, ThreadId};

use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_data_structures::sync::Lock;
use rustc_errors::{DiagInner, MultiSpan};
use rustc_hir::HirId;
use rustc_lint_defs::LintId;
use rustc_span::Span;
use rustc_span::def_id::{DefId, LocalDefId};

use super::QueryJobId;
use super::resident_graph::{Graph, NodeId};
use crate::dep_graph::DepNodeIndex;
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
    trait_tasks: Lock<FxHashMap<ThreadId, Vec<TraitTask>>>,
    active_trait_tasks: AtomicUsize,
    delayed_attribute_lints: Lock<Option<Vec<ResidentDelayedAttributeLint>>>,
    diagnostics: Lock<ResidentDiagnostics<'tcx>>,
    diagnostic_error: AtomicBool,
    pub(crate) source_spans: Lock<FxHashMap<LocalDefId, Span>>,
}

impl Default for ResidentQueryGraph<'_> {
    fn default() -> Self {
        Self {
            graph: Lock::new(Graph::default()),
            trait_tasks: Default::default(),
            active_trait_tasks: AtomicUsize::new(0),
            delayed_attribute_lints: Default::default(),
            diagnostics: Default::default(),
            diagnostic_error: AtomicBool::new(false),
            source_spans: Default::default(),
        }
    }
}

/// Diagnostic outputs and ambient-error effects owned by completed query executions.
#[derive(Default)]
struct ResidentDiagnostics<'tcx> {
    queries: FxHashMap<TaggedQueryKey<'tcx>, Vec<DiagInner>>,
    stashes: FxHashSet<TaggedQueryKey<'tcx>>,
    tainted: FxHashSet<ResidentQueryKey<'tcx>>,
    unowned: bool,
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

struct TraitTask {
    key: NodeId,
    query: Option<QueryJobId>,
    reads: FxHashSet<NodeId>,
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
        }
    }

    pub fn def_id(&self) -> Option<DefId> {
        match self {
            Self::Query(query) => query.resident_key_def_id(),
            Self::HirBody(def_id) | Self::HirStructure(def_id) => Some(def_id.to_def_id()),
            Self::OldTraitSolver(_) | Self::TraitSolver => None,
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
    /// Recomputed queries whose complete outputs were verified unchanged for this revision.
    pub backdated: Vec<TaggedQueryKey<'tcx>>,
    /// Optional diagnostic-only paths before signature backdating.
    pub trace: String,
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
    pub fn record_diagnostic(&self, diagnostic: &DiagInner) {
        if diagnostic.is_error() {
            self.diagnostic_error.store(true, Ordering::Relaxed);
        }
        let owner =
            tls::with_context(|icx| icx.query).and_then(|job| self.graph.lock().running_key(job));
        let mut diagnostics = self.diagnostics.lock();
        if let Some(ResidentQueryKey::Query(owner)) = owner {
            diagnostics.queries.entry(owner).or_default().push(diagnostic.clone());
        } else {
            diagnostics.unowned = true;
        }
    }

    pub fn record_diagnostic_stash(&self, is_error: bool) {
        if is_error {
            self.diagnostic_error.store(true, Ordering::Relaxed);
        }
        let owner =
            tls::with_context(|icx| icx.query).and_then(|job| self.graph.lock().running_key(job));
        let mut diagnostics = self.diagnostics.lock();
        if let Some(ResidentQueryKey::Query(owner)) = owner {
            diagnostics.stashes.insert(owner);
        } else {
            diagnostics.unowned = true;
        }
    }

    /// Keep diagnostic-producing queries conservative while retaining unrelated query values.
    fn diagnostic_inputs(&self) -> Result<Vec<ResidentQueryKey<'tcx>>, &'static str> {
        let diagnostics = self.diagnostics.lock();
        if diagnostics.unowned {
            return Err("resident diagnostic has no query owner");
        }
        Ok(diagnostics
            .queries
            .keys()
            .chain(&diagnostics.stashes)
            .copied()
            .map(ResidentQueryKey::Query)
            .chain(diagnostics.tainted.iter().copied())
            .collect())
    }

    pub fn diagnostic_stats(&self) -> (usize, usize) {
        let diagnostics = self.diagnostics.lock();
        (diagnostics.queries.len(), diagnostics.tainted.len())
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
        let parent = tls::with_context(|icx| icx.query);
        let node = {
            let mut graph = self.graph.lock();
            let node = graph.read(parent, key.into());
            if accesses_value
                && Self::feed_on_read(key)
                && let Some(parent) = parent
            {
                graph.feed(parent, key.into());
            }
            node
        };
        self.read_trait_task(node, parent);
    }

    fn feed_on_read(key: TaggedQueryKey<'tcx>) -> bool {
        // A previous reader can consume these body-level `Steal` values. Restore the producer
        // (and its other readers) whenever a value reader is invalidated. `ensure_ok` and
        // `ensure_result` discard the value and can reuse a previously consumed producer.
        // Crate-level AST `Steal` queries are outside the supported body-edit boundary.
        matches!(
            key,
            TaggedQueryKey::thir_body(_)
                | TaggedQueryKey::mir_built(_)
                | TaggedQueryKey::mir_promoted(_)
                | TaggedQueryKey::mir_drops_elaborated_and_const_checked(_)
                | TaggedQueryKey::mir_coroutine_layout_prepared(_)
        )
    }

    /// Cache indices are unique within this compiler session, including after an eviction.
    pub fn register_cached(&self, index: DepNodeIndex, key: TaggedQueryKey<'tcx>) {
        self.graph.lock().register_cached(
            index.as_u32() as usize,
            key.into(),
            Self::feed_on_read(key),
        );
    }

    /// Record a cache hit without interning and hashing its full query key again.
    /// Return false for an unregistered entry so the caller can use ordinary key recording.
    pub(crate) fn read_cached(&self, index: DepNodeIndex, accesses_value: bool) -> bool {
        let parent = tls::with_context(|icx| icx.query);
        let node = self.graph.lock().read_cached(parent, index.as_u32() as usize, accesses_value);
        let Some(node) = node else { return false };
        self.read_trait_task(node, parent);
        true
    }

    /// Mark access to actual body/interior HIR, separately from the containing owner's signature.
    #[inline]
    pub(crate) fn read_hir_body(&self, owner: LocalDefId) {
        let parent = tls::with_context(|icx| icx.query);
        let node = {
            let mut graph = self.graph.lock();
            let key = match parent.and_then(|parent| graph.running_key(parent)) {
                // These providers traverse bodies to collect structural information. The editing
                // API only changes literals/simple locals and rejects macros, nested items,
                // closures, anonymous constants, generics and added type/lifetime binders.
                // A future edit that can alter any of these must invalidate HirStructure as well.
                Some(ResidentQueryKey::Query(
                    TaggedQueryKey::hir_crate_items(_) | TaggedQueryKey::resolve_bound_vars(_),
                )) => ResidentQueryKey::HirStructure(owner),
                _ => ResidentQueryKey::HirBody(owner),
            };
            graph.read(parent, key)
        };
        self.read_trait_task(node, parent);
    }

    /// Mark access to solver-global caches whose entries are not represented as rustc queries.
    #[inline]
    pub(crate) fn read_trait_solver(&self) {
        if self.diagnostic_error.load(Ordering::Relaxed) {
            self.diagnostics.lock().tainted.insert(ResidentQueryKey::TraitSolver);
        }
        let parent = tls::with_context(|icx| icx.query);
        let node = self.graph.lock().read(parent, ResidentQueryKey::TraitSolver);
        self.read_trait_task(node, parent);
    }

    fn read_trait_task(&self, key: NodeId, query: Option<QueryJobId>) {
        // Each thread increments this before entering a task and decrements it after leaving.
        // A zero value therefore safely skips the thread lookup when no task can own this read.
        if self.active_trait_tasks.load(Ordering::Relaxed) == 0 {
            return;
        }
        if let Some(task) =
            self.trait_tasks.lock().get_mut(&thread::current().id()).and_then(|s| s.last_mut())
            && key != task.key
            && task.query == query
        {
            // A nested provider already records its own inputs. The task read that query before
            // entering its scope; recording its inputs here would bypass unchanged query outputs.
            task.reads.insert(key);
        }
    }

    /// Record a global legacy solver lookup even when it hits an existing answer.
    pub fn read_old_trait_solver(&self, key: ResidentOldTraitKey<'tcx>) {
        let key = ResidentQueryKey::OldTraitSolver(key);
        let parent = tls::with_context(|icx| icx.query);
        let node = self.graph.lock().read(parent, key);
        self.read_trait_task(node, parent);
    }

    /// Capture real query reads and nested global solver lookups on a legacy cache miss.
    pub fn with_old_trait_solver<R>(
        &self,
        key: ResidentOldTraitKey<'tcx>,
        compute: impl FnOnce() -> R,
    ) -> R {
        let key = self.graph.lock().intern_key(ResidentQueryKey::OldTraitSolver(key));
        let thread = thread::current().id();
        let query = tls::with_context(|icx| icx.query);
        self.trait_tasks.lock().entry(thread).or_default().push(TraitTask {
            key,
            query,
            reads: FxHashSet::default(),
        });
        self.active_trait_tasks.fetch_add(1, Ordering::Relaxed);
        let finished = Cell::new(false);
        let _end = rustc_data_structures::defer(|| {
            let task = self.trait_tasks.lock().get_mut(&thread).unwrap().pop().unwrap();
            self.active_trait_tasks.fetch_sub(1, Ordering::Relaxed);
            if finished.get() {
                if self.diagnostic_error.load(Ordering::Relaxed) {
                    let key = self.graph.lock().key(task.key);
                    self.diagnostics.lock().tainted.insert(key);
                }
                self.graph.lock().record_external_nodes(task.key, task.reads);
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
        self.diagnostics.lock().queries.remove(&key);
        let finished = Cell::new(false);
        let _end = rustc_data_structures::defer(|| {
            if finished.get() && self.diagnostic_error.load(Ordering::Relaxed) {
                self.diagnostics.lock().tainted.insert(key.into());
            }
            self.graph.lock().end(job, finished.get());
        });
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

    fn advance_with_backdated(
        &self,
        inputs: impl IntoIterator<Item = ResidentQueryKey<'tcx>>,
        backdated: &FxHashSet<ResidentQueryKey<'tcx>>,
    ) -> ResidentInvalidation<'tcx> {
        let (revision, queries) = self.graph.lock().advance_with_backdated(inputs, backdated);
        ResidentInvalidation { revision, queries: queries.into_iter().collect() }
    }

    /// Inspect all currently recorded transitive readers without advancing the revision.
    pub fn affected(
        &self,
        inputs: impl IntoIterator<Item = ResidentQueryKey<'tcx>>,
    ) -> Vec<ResidentQueryKey<'tcx>> {
        self.graph.lock().closure(inputs).into_iter().collect()
    }

    pub(crate) fn affected_with_backdated(
        &self,
        inputs: impl IntoIterator<Item = ResidentQueryKey<'tcx>>,
        backdated: &FxHashSet<ResidentQueryKey<'tcx>>,
    ) -> Vec<ResidentQueryKey<'tcx>> {
        self.graph.lock().closure_with_backdated(inputs, backdated).into_iter().collect()
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
    /// Validate a result boundary before allowing another source revision.
    pub fn ohm_resident_diagnostics_complete(self) -> Result<(), &'static str> {
        let resident = self.query_system.resident.as_ref().ok_or("resident graph is disabled")?;
        resident.diagnostic_inputs()?;
        if !self.dcx().resident_revision_complete() {
            return Err("resident revision has unresolved diagnostic obligations");
        }
        Ok(())
    }

    /// Discard diagnostic and error-tainted query results before clearing ambient error state.
    ///
    /// # Safety
    /// The caller owns the sole compiler thread between completed requests.
    pub unsafe fn ohm_begin_resident_diagnostic_revision(self) -> Result<(), &'static str> {
        self.ohm_resident_diagnostics_complete()?;
        let resident = self.query_system.resident.as_ref().unwrap();
        let mut inputs = resident.diagnostic_inputs()?;
        // Expectations consume ambient fulfillment IDs, including suppressed diagnostics.
        inputs.push(TaggedQueryKey::check_expectations(None).into());
        unsafe { self.ohm_invalidate_resident_queries(inputs) }?;
        unsafe { self.dcx().reset_resident_revision() }?;
        *resident.diagnostics.lock() = ResidentDiagnostics::default();
        resident.diagnostic_error.store(false, Ordering::Relaxed);
        Ok(())
    }

    /// Explain invalidation before adding signature barriers, without invoking query providers.
    pub(crate) fn ohm_resident_invalidation_trace(
        self,
        inputs: &[ResidentQueryKey<'tcx>],
        backdated: &[TaggedQueryKey<'tcx>],
        function: LocalDefId,
    ) -> String {
        use std::fmt::Write;

        if std::env::var_os("OHM_RESIDENT_TRACE").is_none_or(|value| value != "1") {
            return String::new();
        }
        let Some(resident) = &self.query_system.resident else {
            return String::new();
        };
        let graph = resident.graph.lock();
        let barriers: FxHashSet<_> = backdated.iter().copied().map(Into::into).collect();
        let opaque = graph.shortest_path(inputs.iter().copied(), &barriers, |key| {
            matches!(
                key,
                ResidentQueryKey::Query(
                    TaggedQueryKey::type_of_opaque(_)
                        | TaggedQueryKey::type_of_opaque_hir_typeck(_)
                )
            )
        });
        let solver_reset = opaque.is_some();
        let typeck_inputs =
            if solver_reset { vec![ResidentQueryKey::TraitSolver] } else { inputs.to_vec() };
        let unrelated = |key, outside_module: bool| {
            let ResidentQueryKey::Query(TaggedQueryKey::typeck_root(owner)) = key else {
                return false;
            };
            owner != function
                && (!outside_module
                    || !self
                        .hir_def_path(owner)
                        .to_string_no_crate_verbose()
                        .contains("empty_state_animation"))
        };
        let typeck = graph
            .shortest_path(typeck_inputs.iter().copied(), &barriers, |key| unrelated(key, true))
            .or_else(|| {
                graph.shortest_path(typeck_inputs.iter().copied(), &barriers, |key| {
                    unrelated(key, false)
                })
            });
        let render = |path: Option<Vec<(ResidentQueryKey<'tcx>, bool, bool)>>| {
            let Some(path) = path else { return "none".to_owned() };
            let mut output = String::new();
            for (index, &(key, read, feed)) in path.iter().take(32).enumerate() {
                if index != 0 {
                    output.push_str(match (read, feed) {
                        (true, true) => " -read+feed-> ",
                        (true, false) => " -read-> ",
                        (false, true) => " -feed-> ",
                        (false, false) => unreachable!(),
                    });
                }
                output.push_str(key.name());
                if let Some(def_id) = key.def_id() {
                    // `def_path` reads immutable definition tables, not query-backed pretty-printing.
                    write!(output, "[{}]", self.def_path(def_id).to_string_no_crate_verbose())
                        .unwrap();
                }
            }
            if path.len() > 32 {
                write!(output, " ... ({} total nodes)", path.len()).unwrap();
            }
            output
        };
        format!(
            "pre_signature_backdated={}; inputs_to_opaque={}; {}_to_unrelated_typeck={}",
            backdated.len(),
            render(opaque),
            if solver_reset { "solver_reset" } else { "inputs" },
            render(typeck),
        )
    }

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
        // SAFETY: This preserves the caller's quiescence contract without backdating any queries.
        unsafe { self.ohm_invalidate_resident_queries_with_backdated(inputs, &[]) }
    }

    /// Invalidate readers, stopping only at independently revalidated, unchanged query outputs.
    ///
    /// # Safety
    /// In addition to quiescence, each backdated query must have been recomputed from current
    /// inputs, have exactly the same complete output, and have no changed diagnostic or feed side
    /// effects. Its current value must remain cached, with dependencies valid for its current inputs.
    pub(crate) unsafe fn ohm_invalidate_resident_queries_with_backdated(
        self,
        inputs: impl IntoIterator<Item = ResidentQueryKey<'tcx>>,
        backdated: &[TaggedQueryKey<'tcx>],
    ) -> Result<ResidentEviction<'tcx>, &'static str> {
        if self.dep_graph.is_fully_enabled() || self.sess.opts.jobs.frontend.is_some() {
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
        let backdated_inputs: FxHashSet<_> = backdated.iter().copied().map(Into::into).collect();
        if inputs.iter().any(|input| backdated_inputs.contains(input)) {
            return Err("changed inputs cannot be backdated");
        }
        // The next trait solver does not yet provide individually keyed resident dependencies.
        // Legacy solver answers are reached through the real queries and nested solver goals that
        // produced them, including when a later query uses an already cached solver answer.
        if graph.affected_with_backdated(inputs.iter().copied(), &backdated_inputs).iter().any(
            |key| {
                matches!(
                    key,
                    ResidentQueryKey::Query(
                        TaggedQueryKey::type_of_opaque(_)
                            | TaggedQueryKey::type_of_opaque_hir_typeck(_)
                    )
                )
            },
        ) {
            inputs.push(ResidentQueryKey::TraitSolver);
        }
        let plan = graph.advance_with_backdated(inputs, &backdated_inputs);
        let mut eviction = ResidentEviction {
            revision: plan.revision,
            inputs: Vec::new(),
            structural_inputs: Vec::new(),
            removed: Vec::new(),
            uncached: Vec::new(),
            backdated: backdated.to_vec(),
            trace: String::new(),
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
