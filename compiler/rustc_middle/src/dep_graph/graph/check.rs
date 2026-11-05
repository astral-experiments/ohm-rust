//! Reuses checked frontend values while retaining the native build's dependency graph.

use std::sync::OnceLock;

use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_data_structures::memmap::Mmap;

use super::*;
use crate::dep_graph::serialized::IndexedDepGraph;
use crate::query::on_disk_cache::OnDiskCache;

/// A compatible check checkpoint, mapped without decoding its graph or query tables.
pub struct CheckCacheSource {
    graph: Mmap,
    graph_start: usize,
    cache: Mmap,
    cache_start: usize,
}

impl CheckCacheSource {
    /// Both headers and the graph's argument hash must have been validated.
    pub fn new(graph: Mmap, graph_start: usize, cache: Mmap, cache_start: usize) -> Option<Self> {
        IndexedDepGraph::new(&graph, graph_start)?;
        Some(Self { graph, graph_start, cache, cache_start })
    }
}

pub(super) struct CheckCache {
    graph: Lock<Option<Arc<Mmap>>>,
    graph_start: usize,
    cache_source: Lock<Option<(Mmap, usize)>>,
    cache: OnceLock<Option<OnDiskCache>>,
    /// Green build nodes already have a usable fingerprint in the build graph.
    /// Retain current fingerprints only for recomputed red nodes and new nodes.
    current: Lock<FxHashMap<DepNode, (Option<Fingerprint>, DepNodeIndex)>>,
}

#[derive(Default)]
struct CheckedDependencies {
    active: FxHashSet<SerializedDepNodeIndex>,
    validated: FxHashSet<SerializedDepNodeIndex>,
    edges: Vec<DepNodeIndex>,
    features: Vec<Symbol>,
}

impl CheckCache {
    pub(super) fn new(source: CheckCacheSource) -> Self {
        Self {
            graph: Lock::new(Some(Arc::new(source.graph))),
            graph_start: source.graph_start,
            cache_source: Lock::new(Some((source.cache, source.cache_start))),
            cache: OnceLock::new(),
            current: Lock::default(),
        }
    }

    fn cache(&self, sess: &Session) -> Option<&OnDiskCache> {
        self.cache
            .get_or_init(|| {
                let (cache, start) = self.cache_source.lock().take()?;
                OnDiskCache::new(sess, cache, start).ok()
            })
            .as_ref()
    }

    pub(super) fn record(
        &self,
        node: DepNode,
        fingerprint: Option<Fingerprint>,
        index: DepNodeIndex,
    ) {
        self.current.lock().insert(node, (fingerprint, index));
    }

    fn close(&self) {
        self.graph.lock().take();
        self.cache_source.lock().take();
        if let Some(Some(cache)) = self.cache.get() {
            cache.close_serialized_data_mmap();
        }
    }
}

impl DepGraph {
    /// Releases the check mmap before the current session replaces its cache files.
    pub fn close_latest_check_cache(&self) {
        if let Some(data) = self.data()
            && let Some(check) = &data.latest_check
        {
            check.close();
        }
    }
}

impl DepGraphData {
    /// Loads a checked value only after validating its dependencies against this build.
    /// The resulting node records the validated dependency frontier in the build graph;
    /// no check nodes or indices are grafted into the build's previous graph.
    pub fn try_reuse_latest_check<'tcx, R>(
        &self,
        tcx: TyCtxt<'tcx>,
        node: DepNode,
        hash_result: Option<fn(&mut StableHashState<'_>, &R) -> Fingerprint>,
        verify: bool,
        load: impl FnOnce(&OnDiskCache, SerializedDepNodeIndex) -> Option<R>,
    ) -> Option<(R, DepNodeIndex)> {
        let check = self.latest_check.as_ref()?;
        let data = Arc::clone(check.graph.lock().as_ref()?);
        let graph = IndexedDepGraph::new(&data, check.graph_start)?;
        let index = graph.node_to_index_opt(&node)?;
        let root = graph.node(index)?;
        let cache = check.cache(tcx.sess)?;
        if !cache.loadable_from_disk(index) {
            return None;
        }
        tracing::info!(?node, "ohm_latest_check_candidate");
        let _timer = tcx.prof.generic_activity("incr_comp_reuse_latest_check");
        let mut dependencies = tcx.dep_graph.with_ignore(|| {
            let mut dependencies = CheckedDependencies::default();
            for dependency in root.edges() {
                self.validate_checked_dependency(
                    tcx,
                    check,
                    cache,
                    &graph,
                    dependency,
                    &mut dependencies,
                )?;
            }
            Some(dependencies)
        })?;
        let value = load(cache, index)?;
        if verify {
            let fingerprint = hash_result.map_or(Fingerprint::ZERO, |hash| {
                tcx.with_stable_hashing_context(|mut hcx| hash(&mut hcx, &value))
            });
            assert_eq!(fingerprint, root.fingerprint);
        }
        self.assert_dep_node_not_yet_allocated_in_current_session(tcx.sess, &node, || {
            format!("reusing a checked query with an existing dep node: {node:?}")
        });
        for symbol in dependencies.features {
            let index = self.encode_side_effect(tcx, QuerySideEffect::CheckFeature { symbol });
            tcx.sess.used_features.lock().insert(symbol, index.as_u32());
            dependencies.edges.push(index);
        }
        // The checked value already has a stable hash. Its dependencies were
        // validated above; retain ordinary sampled verification when decoding.
        let fingerprint = hash_result.map(|_| root.fingerprint);
        let current = self.alloc_and_color_node(node, &dependencies.edges, fingerprint);
        if tcx.sess.opts.unstable_opts.query_dep_graph {
            self.mark_debug_loaded_from_disk(node);
        }
        tracing::info!(
            kind = ?node.kind,
            dependency_nodes = dependencies.validated.len(),
            node_reads = graph.node_reads(),
            graph_nodes = graph.node_count(),
            "ohm_latest_check_reuse"
        );
        Some((value, current))
    }

    /// Stops at current values with matching hashes; otherwise validates the checked
    /// computation's inputs in their original order. Unknown side effects and inputs
    /// that cannot be compared cause an ordinary provider invocation instead.
    fn validate_checked_dependency<'tcx>(
        &self,
        tcx: TyCtxt<'tcx>,
        check: &CheckCache,
        cache: &OnDiskCache,
        graph: &IndexedDepGraph<'_>,
        index: SerializedDepNodeIndex,
        dependencies: &mut CheckedDependencies,
    ) -> Option<()> {
        if dependencies.validated.contains(&index) {
            return Some(());
        }
        // Bound speculative validation and conservatively reject cycles.
        if dependencies.active.len() >= 128
            || dependencies.validated.len() >= 8192
            || !dependencies.active.insert(index)
        {
            return None;
        }
        let entry = graph.node(index)?;
        let node = entry.node;
        if node.kind == DepKind::SideEffect {
            // Replay feature-use bookkeeping only after the entire checked result
            // has been validated. Diagnostics conservatively use the provider.
            let QuerySideEffect::CheckFeature { symbol } = cache.load_side_effect(tcx, index)?
            else {
                return None;
            };
            dependencies.features.push(symbol);
            dependencies.active.remove(&index);
            dependencies.validated.insert(index);
            return Some(());
        }
        if matches!(node.kind, DepKind::Null | DepKind::Red) {
            tracing::info!(?node, reason = "untracked dependency", "ohm_latest_check_miss");
            return None;
        }
        let expected = entry.fingerprint;
        // Anonymous keys are session-local. Validate their edges rather than matching
        // their key against a separately saved build graph.
        // A zero fingerprint may denote a `no_hash` query. Equal placeholders
        // cannot establish that two independently saved query results agree.
        if expected != Fingerprint::ZERO
            && tcx.key_fingerprint_style(node.kind).is_maybe_recoverable()
        {
            if let Some((Some(actual), current)) = check.current.lock().get(&node).copied() {
                if actual != expected {
                    tracing::info!(
                        ?node,
                        reason = "changed current input",
                        "ohm_latest_check_miss"
                    );
                    return None;
                }
                dependencies.edges.push(current);
                dependencies.active.remove(&index);
                dependencies.validated.insert(index);
                return Some(());
            }
            if let Some(previous) = self.previous.node_to_index_opt(&node) {
                // Check and build metadata can have different whole-crate hashes
                // while a particular external query still returns the same value.
                // Validate that query against the native graph before comparing its
                // result. Never force unknown local computations: that can redo
                // the frontend work we are trying to reuse, or introduce cycles.
                let external = node.extract_def_id(tcx).is_some_and(|id| !id.is_local());
                if external && !tcx.is_eval_always(node.kind) {
                    self.try_mark_green(tcx, &node);
                }
                if (external || tcx.is_eval_always(node.kind))
                    && matches!(self.colors.get(previous), DepNodeColor::Unknown)
                {
                    let frame = MarkFrame { index: previous, parent: None };
                    tcx.try_force_from_dep_node(node, previous, None, &frame);
                }
                if let Some((Some(actual), current)) = check.current.lock().get(&node).copied() {
                    if actual != expected {
                        tracing::info!(
                            ?node,
                            reason = "changed forced input",
                            "ohm_latest_check_miss"
                        );
                        return None;
                    }
                    dependencies.edges.push(current);
                    dependencies.active.remove(&index);
                    dependencies.validated.insert(index);
                    return Some(());
                }
                if let DepNodeColor::Green(current) = self.colors.get(previous) {
                    if self.previous.value_fingerprint_for_index(previous) != expected {
                        tracing::info!(
                            ?node,
                            reason = "changed green input",
                            "ohm_latest_check_miss"
                        );
                        return None;
                    }
                    dependencies.edges.push(current);
                    dependencies.active.remove(&index);
                    dependencies.validated.insert(index);
                    return Some(());
                }
            }
        }
        if tcx.is_eval_always(node.kind) {
            tracing::info!(?node, reason = "unvalidated input", "ohm_latest_check_miss");
            return None;
        }
        for dependency in entry.edges() {
            self.validate_checked_dependency(tcx, check, cache, graph, dependency, dependencies)?;
        }
        dependencies.active.remove(&index);
        dependencies.validated.insert(index);
        Some(())
    }
}
