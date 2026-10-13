//! Reuses checked frontend values while retaining the native build's dependency graph.

use std::sync::OnceLock;

use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_data_structures::memmap::Mmap;
use rustc_serialize::opaque::MemDecoder;

use super::*;
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
    pub fn new(graph: Mmap, graph_start: usize, cache: Mmap, cache_start: usize) -> Self {
        Self { graph, graph_start, cache, cache_start }
    }
}

pub(super) struct CheckCache {
    source: Lock<Option<CheckCacheSource>>,
    decoded: OnceLock<Option<CheckedQueries>>,
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

struct CheckedQueries {
    graph: Arc<SerializedDepGraph>,
    cache: OnDiskCache,
}

impl CheckCache {
    pub(super) fn new(source: CheckCacheSource) -> Self {
        Self { source: Lock::new(Some(source)), decoded: OnceLock::new(), current: Lock::default() }
    }

    fn get(&self, sess: &Session) -> Option<&CheckedQueries> {
        self.decoded
            .get_or_init(|| {
                let source = self.source.lock().take()?;
                let mut decoder = MemDecoder::new(&source.graph, source.graph_start).ok()?;
                let graph = SerializedDepGraph::decode(&mut decoder, &sess.prof);
                let cache = OnDiskCache::new(sess, source.cache, source.cache_start).ok()?;
                Some(CheckedQueries { graph, cache })
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
        self.source.lock().take();
        if let Some(Some(checked)) = self.decoded.get() {
            checked.cache.close_serialized_data_mmap();
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
        let checked = check.get(tcx.sess)?;
        let index = checked.graph.node_to_index_opt(&node)?;
        if !checked.cache.loadable_from_disk(index) {
            return None;
        }
        tracing::info!(?node, "ohm_latest_check_candidate");
        let _timer = tcx.prof.generic_activity("incr_comp_reuse_latest_check");
        let mut dependencies = tcx.dep_graph.with_ignore(|| {
            let mut dependencies = CheckedDependencies::default();
            for dependency in checked.graph.edge_targets_from(index) {
                self.validate_checked_dependency(
                    tcx,
                    check,
                    checked,
                    dependency,
                    &mut dependencies,
                )?;
            }
            Some(dependencies)
        })?;
        let value = load(&checked.cache, index)?;
        if verify {
            let fingerprint = hash_result.map_or(Fingerprint::ZERO, |hash| {
                tcx.with_stable_hashing_context(|mut hcx| hash(&mut hcx, &value))
            });
            assert_eq!(fingerprint, checked.graph.value_fingerprint_for_index(index));
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
        let fingerprint = hash_result.map(|_| checked.graph.value_fingerprint_for_index(index));
        let current = self.alloc_and_color_node(node, &dependencies.edges, fingerprint);
        if tcx.sess.opts.unstable_opts.query_dep_graph {
            self.mark_debug_loaded_from_disk(node);
        }
        tracing::info!(kind = ?node.kind, "ohm_latest_check_reuse");
        Some((value, current))
    }

    /// Stops at current values with matching hashes; otherwise validates the checked
    /// computation's inputs in their original order. Unknown side effects and inputs
    /// that cannot be compared cause an ordinary provider invocation instead.
    fn validate_checked_dependency<'tcx>(
        &self,
        tcx: TyCtxt<'tcx>,
        check: &CheckCache,
        checked: &CheckedQueries,
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
        let node = *checked.graph.index_to_node(index);
        if node.kind == DepKind::SideEffect {
            // Replay feature-use bookkeeping only after the entire checked result
            // has been validated. Diagnostics conservatively use the provider.
            let QuerySideEffect::CheckFeature { symbol } =
                checked.cache.load_side_effect(tcx, index)?
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
        let expected = checked.graph.value_fingerprint_for_index(index);
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
                // Force only input nodes. Walking the old build's dependencies
                // here can recompute the very frontend result we want to reuse.
                if tcx.is_eval_always(node.kind)
                    && matches!(self.colors.get(previous), DepNodeColor::Unknown)
                {
                    let frame = MarkFrame { index: previous, parent: None };
                    tcx.try_force_from_dep_node(node, previous, &frame);
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
        for dependency in checked.graph.edge_targets_from(index) {
            self.validate_checked_dependency(tcx, check, checked, dependency, dependencies)?;
        }
        dependencies.active.remove(&index);
        dependencies.validated.insert(index);
        Some(())
    }
}
