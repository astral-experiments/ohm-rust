use std::hash::Hash;
use std::num::NonZeroU32;

use rustc_data_structures::fx::{FxHashMap as HashMap, FxHashSet as HashSet, FxIndexSet};
use smallvec::SmallVec;

// Keys can contain large query arguments. Store them once and use compact IDs in every edge.
pub(crate) type NodeId = u32;
type EdgeList = SmallVec<[NodeId; 4]>;

#[derive(Clone, Copy)]
struct CachedRead<J> {
    job: J,
    index: usize,
    node: NodeId,
    accesses_value: bool,
}

/// A bounded, thread-local cache of dependencies already recorded for a query job.
/// Query job IDs and virtual cache indices are never reused within a compiler session.
pub(crate) struct CachedReads<J> {
    session: u64,
    entries: [Option<CachedRead<J>>; 128],
}

impl<J: Copy + Eq> CachedReads<J> {
    pub(crate) const fn new() -> Self {
        Self { session: 0, entries: [None; 128] }
    }

    #[inline]
    pub(crate) fn read(
        &mut self,
        session: u64,
        job: J,
        index: usize,
        accesses_value: bool,
        record: impl FnOnce() -> Option<NodeId>,
    ) -> Option<NodeId> {
        if self.session != session {
            self.entries.fill(None);
            self.session = session;
        }
        let slot = index % self.entries.len();
        if let Some(entry) = self.entries[slot]
            && entry.job == job
            && entry.index == index
            && (entry.accesses_value || !accesses_value)
        {
            return Some(entry.node);
        }
        let node = record()?;
        self.entries[slot] = Some(CachedRead { job, index, node, accesses_value });
        Some(node)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct CachedNode(NonZeroU32);

impl CachedNode {
    const FEED: u32 = 1 << 31;

    fn new(key: NodeId, feed_on_read: bool) -> Self {
        // Reserve zero for an unregistered index and the high bit for consumed-value reads.
        // This limit is far beyond the prototype's retention budget with full query keys.
        assert!(key < Self::FEED - 1, "resident cached node index overflow");
        Self(NonZeroU32::new((key + 1) | if feed_on_read { Self::FEED } else { 0 }).unwrap())
    }

    fn key(self) -> NodeId {
        (self.0.get() & (Self::FEED - 1)) - 1
    }

    fn feed_on_read(self) -> bool {
        self.0.get() & Self::FEED != 0
    }
}

struct Running {
    key: NodeId,
    reads: EdgeList,
    feeds: EdgeList,
}

pub(crate) struct Graph<K, J> {
    keys: FxIndexSet<K>,
    recent_key: Option<NodeId>,
    running: HashMap<J, Running>,
    reads: Vec<Option<EdgeList>>,
    feeds: Vec<EdgeList>,
    reverse: Vec<EdgeList>,
    cached_nodes: Vec<Option<CachedNode>>,
    revision: u64,
}

impl<K, J> Default for Graph<K, J> {
    fn default() -> Self {
        Self {
            keys: FxIndexSet::default(),
            recent_key: None,
            running: HashMap::default(),
            reads: Vec::new(),
            feeds: Vec::new(),
            reverse: Vec::new(),
            cached_nodes: Vec::new(),
            revision: 0,
        }
    }
}

// These sets represent a conservative mathematical closure; traversal and update ordering cannot
// change its membership or the number of edges. The external API documents that order is absent.
#[expect(rustc::potential_query_instability)]
impl<K: Copy + Eq + Hash, J: Copy + Eq + Hash> Graph<K, J> {
    fn intern(keys: &mut FxIndexSet<K>, recent: &mut Option<NodeId>, key: K) -> NodeId {
        if let Some(index) = *recent
            && keys[index as usize] == key
        {
            return index;
        }
        let (index, _) = keys.insert_full(key);
        let index = index.try_into().expect("resident graph node index overflow");
        *recent = Some(index);
        index
    }

    fn allocate_edges(&mut self) {
        let len = self.keys.len();
        self.reads.resize_with(len, || None);
        self.feeds.resize_with(len, EdgeList::new);
        self.reverse.resize_with(len, EdgeList::new);
    }

    pub(crate) fn intern_key(&mut self, key: K) -> NodeId {
        Self::intern(&mut self.keys, &mut self.recent_key, key)
    }

    pub(crate) fn key(&self, node: NodeId) -> K {
        self.keys[node as usize]
    }

    fn insert_edge(edges: &mut EdgeList, key: NodeId) -> bool {
        if edges.last().is_none_or(|&last| last < key) {
            edges.push(key);
            true
        } else if let Err(index) = edges.binary_search(&key) {
            edges.insert(index, key);
            true
        } else {
            false
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn running_key(&self, job: J) -> Option<K> {
        self.running.get(&job).map(|running| self.keys[running.key as usize])
    }

    pub(crate) fn begin(&mut self, job: J, key: K) {
        let key = Self::intern(&mut self.keys, &mut self.recent_key, key);
        assert!(
            self.running
                .insert(job, Running { key, reads: EdgeList::new(), feeds: EdgeList::new() })
                .is_none(),
            "resident query job already active"
        );
    }

    pub(crate) fn read(&mut self, job: Option<J>, key: K) -> NodeId {
        let key = self.intern_key(key);
        if let Some(running) = job.and_then(|job| self.running.get_mut(&job)) {
            if running.key != key && running.reads.last() != Some(&key) {
                running.reads.push(key);
            }
        }
        key
    }

    /// Associate rustc's unique virtual query index with its already recorded graph node.
    pub(crate) fn register_cached(&mut self, index: usize, key: K, feed_on_read: bool) {
        let key = CachedNode::new(self.intern_key(key), feed_on_read);
        if self.cached_nodes.len() <= index {
            self.cached_nodes.resize(index + 1, None);
        }
        let previous = self.cached_nodes[index].replace(key);
        assert!(previous.is_none_or(|previous| previous == key));
    }

    #[cfg(test)]
    fn cached_key(&self, index: usize) -> Option<K> {
        self.cached_nodes.get(index).copied().flatten().map(|node| self.key(node.key()))
    }

    pub(crate) fn read_cached(
        &mut self,
        job: Option<J>,
        index: usize,
        accesses_value: bool,
    ) -> Option<NodeId> {
        let node = self.cached_nodes.get(index).copied().flatten()?;
        let key = node.key();
        if let Some(running) = job.and_then(|job| self.running.get_mut(&job))
            && running.key != key
        {
            if running.reads.last() != Some(&key) {
                running.reads.push(key);
            }
            // An ensure followed by a value read must still restore consumed prerequisites.
            if accesses_value && node.feed_on_read() && running.feeds.last() != Some(&key) {
                running.feeds.push(key);
            }
        }
        Some(key)
    }

    pub(crate) fn feed(&mut self, job: J, key: K) {
        if let Some(running) = self.running.get_mut(&job) {
            let key = Self::intern(&mut self.keys, &mut self.recent_key, key);
            if running.key != key && running.feeds.last() != Some(&key) {
                running.feeds.push(key);
            }
        }
    }

    /// Preserve reads of a memoized computation whose lifetime is independent of a query job.
    /// Multiple computations of the same key conservatively retain their combined read sets.
    pub(crate) fn record_external_nodes(
        &mut self,
        consumer: NodeId,
        inputs: impl IntoIterator<Item = NodeId>,
    ) {
        self.allocate_edges();
        for input in inputs {
            if consumer != input {
                let reads = self.reads[consumer as usize].get_or_insert_default();
                if Self::insert_edge(reads, input) {
                    Self::insert_edge(&mut self.reverse[input as usize], consumer);
                }
            }
        }
    }

    #[cfg(test)]
    fn record_external(&mut self, consumer: K, inputs: impl IntoIterator<Item = K>) {
        let consumer = self.intern_key(consumer);
        let inputs: EdgeList = inputs.into_iter().map(|key| self.intern_key(key)).collect();
        self.record_external_nodes(consumer, inputs);
    }

    pub(crate) fn end(&mut self, job: J, commit: bool) {
        let mut running = self.running.remove(&job).expect("resident query job not active");
        if !commit {
            return;
        }
        running.reads.sort_unstable();
        running.reads.dedup();
        running.reads.shrink_to_fit();
        running.feeds.sort_unstable();
        running.feeds.dedup();
        running.feeds.shrink_to_fit();
        self.allocate_edges();
        let old_reads = self.reads[running.key as usize].replace(running.reads);
        let old_feeds = std::mem::replace(&mut self.feeds[running.key as usize], running.feeds);
        if let Some(old_reads) = old_reads {
            for key in old_reads {
                self.refresh(key, running.key);
            }
        }
        for key in old_feeds {
            self.refresh(running.key, key);
        }
        // Avoid cloning the sets to appease the borrow checker; the forward graph is already
        // installed, so newly appearing edges can simply be added to the separate reverse graph.
        for &key in self.reads[running.key as usize].as_ref().unwrap() {
            Self::insert_edge(&mut self.reverse[key as usize], running.key);
        }
        for &key in &self.feeds[running.key as usize] {
            Self::insert_edge(&mut self.reverse[running.key as usize], key);
        }
        self.recent_key = Some(running.key);
    }

    fn refresh(&mut self, input: NodeId, consumer: NodeId) {
        let read = self.reads[consumer as usize]
            .as_ref()
            .is_some_and(|reads| reads.binary_search(&input).is_ok());
        let fed = self.feeds[input as usize].binary_search(&consumer).is_ok();
        if !read
            && !fed
            && let Ok(index) = self.reverse[input as usize].binary_search(&consumer)
        {
            self.reverse[input as usize].remove(index);
        }
    }

    pub(crate) fn closure(&self, inputs: impl IntoIterator<Item = K>) -> HashSet<K> {
        self.closure_with_backdated(inputs, &HashSet::default())
    }

    /// Stop propagation through computations whose complete output has been revalidated unchanged.
    pub(crate) fn closure_with_backdated(
        &self,
        inputs: impl IntoIterator<Item = K>,
        backdated: &HashSet<K>,
    ) -> HashSet<K> {
        assert!(self.running.is_empty(), "resident graph can only be inspected between query jobs");
        // Unseen inputs are still part of the result, but need no graph traversal.
        let mut found: HashSet<K> = inputs.into_iter().collect();
        assert!(found.is_disjoint(backdated), "changed inputs cannot be backdated");
        let backdated: HashSet<NodeId> = backdated
            .iter()
            .filter_map(|key| self.keys.get_index_of(key).map(|index| index as NodeId))
            .collect();
        let mut visited: HashSet<NodeId> = found
            .iter()
            .filter_map(|key| self.keys.get_index_of(key).map(|index| index as NodeId))
            .collect();
        let mut pending: Vec<NodeId> = visited.iter().copied().collect();
        while let Some(input) = pending.pop() {
            if let Some(consumers) = self.reverse.get(input as usize) {
                for &consumer in consumers {
                    if !backdated.contains(&consumer) && visited.insert(consumer) {
                        pending.push(consumer);
                    }
                }
            }
        }
        found.extend(visited.into_iter().map(|id| self.keys[id as usize]));
        found
    }

    pub(crate) fn is_quiescent(&self) -> bool {
        self.running.is_empty()
    }

    pub(crate) fn advance(&mut self, inputs: impl IntoIterator<Item = K>) -> (u64, HashSet<K>) {
        self.advance_with_backdated(inputs, &HashSet::default())
    }

    pub(crate) fn advance_with_backdated(
        &mut self,
        inputs: impl IntoIterator<Item = K>,
        backdated: &HashSet<K>,
    ) -> (u64, HashSet<K>) {
        let closure = self.closure_with_backdated(inputs, backdated);
        self.revision = self.revision.checked_add(1).expect("resident graph revision overflow");
        (self.revision, closure)
    }

    pub(crate) fn dependencies(&self, key: K) -> HashSet<K> {
        assert!(self.running.is_empty(), "resident graph can only be inspected between query jobs");
        let Some(key) = self.keys.get_index_of(&key).map(|index| index as NodeId) else {
            return HashSet::default();
        };
        self.reverse
            .iter()
            .enumerate()
            .filter_map(|(input, consumers)| {
                consumers.binary_search(&key).is_ok().then_some(self.keys[input])
            })
            .collect()
    }

    /// Inspect one shortest invalidation path without changing graph state. The booleans identify
    /// read and feed edges from the preceding node; both are false for the initial input.
    pub(crate) fn shortest_path(
        &self,
        inputs: impl IntoIterator<Item = K>,
        backdated: &HashSet<K>,
        mut target: impl FnMut(K) -> bool,
    ) -> Option<Vec<(K, bool, bool)>> {
        assert!(self.running.is_empty(), "resident graph can only be inspected between query jobs");
        let mut predecessors = HashMap::default();
        let mut pending = Vec::new();
        for key in inputs {
            assert!(!backdated.contains(&key), "changed inputs cannot be backdated");
            if target(key) {
                return Some(vec![(key, false, false)]);
            }
            if let Some(index) = self.keys.get_index_of(&key) {
                let id = index as NodeId;
                if predecessors.insert(id, None).is_none() {
                    pending.push(id);
                }
            }
        }
        let mut cursor = 0;
        while let Some(&input) = pending.get(cursor) {
            cursor += 1;
            if target(self.keys[input as usize]) {
                let mut path = Vec::new();
                let mut current = input;
                loop {
                    let previous = predecessors[&current];
                    let (read, feed) = previous.map_or((false, false), |previous| {
                        (
                            self.reads[current as usize]
                                .as_ref()
                                .is_some_and(|reads| reads.binary_search(&previous).is_ok()),
                            self.feeds[previous as usize].binary_search(&current).is_ok(),
                        )
                    });
                    path.push((self.keys[current as usize], read, feed));
                    let Some(previous) = previous else { break };
                    current = previous;
                }
                path.reverse();
                return Some(path);
            }
            if let Some(consumers) = self.reverse.get(input as usize) {
                for &consumer in consumers {
                    if !backdated.contains(&self.keys[consumer as usize])
                        && !predecessors.contains_key(&consumer)
                    {
                        predecessors.insert(consumer, Some(input));
                        pending.push(consumer);
                    }
                }
            }
        }
        None
    }

    pub(crate) fn stats(&self) -> (u64, usize, usize) {
        assert!(self.running.is_empty(), "resident graph can only be inspected between query jobs");
        let edges = self.reverse.iter().map(EdgeList::len).sum();
        (self.revision, self.reads.iter().filter(|reads| reads.is_some()).count(), edges)
    }
}

#[cfg(test)]
#[path = "resident_graph/tests.rs"]
mod tests;
