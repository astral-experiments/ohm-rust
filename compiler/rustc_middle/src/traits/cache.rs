//! Cache for candidate selection.

use std::hash::Hash;

use rustc_data_structures::fx::FxHashMap;
use rustc_data_structures::sync::Lock;

use super::SelectionResult;
use super::select::SelectionCandidate;
use crate::dep_graph::{DepKind, DepNode, DepNodeIndex};
use crate::ty::{self, TyCtxt};

pub struct WithDepNodeCache<Key, Value> {
    hashmap: Lock<FxHashMap<Key, WithDepNode<Value>>>,
}

impl<Key: Clone, Value: Clone> Clone for WithDepNodeCache<Key, Value> {
    fn clone(&self) -> Self {
        Self { hashmap: Lock::new(self.hashmap.borrow().clone()) }
    }
}

impl<Key, Value> Default for WithDepNodeCache<Key, Value> {
    fn default() -> Self {
        Self { hashmap: Default::default() }
    }
}

impl<Key: Eq + Hash, Value: Clone> WithDepNodeCache<Key, Value> {
    pub fn get<'tcx>(&self, key: &Key, tcx: TyCtxt<'tcx>) -> Option<Value> {
        Some(self.hashmap.borrow().get(key)?.get(tcx))
    }

    pub fn insert(&self, key: Key, dep_node: DepNodeIndex, value: Value) {
        self.hashmap.borrow_mut().insert(key, WithDepNode::new(dep_node, value));
    }
}

impl<'tcx> TyCtxt<'tcx> {
    /// The first caller reads the anonymous selection task directly. Later callers
    /// only need the cached answer. Give that answer a stable identity so that
    /// selecting the same impl after a change to the candidate list can keep
    /// those callers green. The same applies when selection remains ambiguous.
    pub fn cache_global_candidate(
        self,
        key: (ty::TypingEnv<'tcx>, ty::TraitClause<'tcx>),
        task: DepNodeIndex,
        result: SelectionResult<'tcx, SelectionCandidate<'tcx>>,
    ) {
        let cache = &self.caches.selection_cache;
        let Some(graph) = self.dep_graph.data() else {
            return cache.insert(key, task, result);
        };
        let answer = match &result {
            Ok(None) => None,
            Ok(Some(SelectionCandidate::ImplCandidate(id))) => Some(*id),
            _ => return cache.insert(key, task, result),
        };
        let node = DepNode::construct(self, DepKind::CachedTraitSelection, &(key, answer));
        let mut cache = cache.hashmap.borrow_mut();
        let entry = cache.entry(key);
        // Concurrent producers can compute this goal independently. Create one
        // named node and keep each replacement tied to the task producing it.
        let dep_node = if matches!(entry, std::collections::hash_map::Entry::Vacant(_)) {
            graph.record_cache_entry(node, task)
        } else {
            task
        };
        entry.insert_entry(WithDepNode::new(dep_node, result));
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WithDepNode<T> {
    dep_node: DepNodeIndex,
    cached_value: T,
}

impl<T: Clone> WithDepNode<T> {
    pub(crate) fn new(dep_node: DepNodeIndex, cached_value: T) -> Self {
        WithDepNode { dep_node, cached_value }
    }

    pub(crate) fn get<'tcx>(&self, tcx: TyCtxt<'tcx>) -> T {
        tcx.dep_graph.read_index(self.dep_node);
        self.cached_value.clone()
    }
}
