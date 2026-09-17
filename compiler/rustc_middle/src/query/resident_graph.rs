#[cfg(test)]
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

#[cfg(not(test))]
use rustc_data_structures::fx::{FxHashMap as HashMap, FxHashSet as HashSet};

struct Running<K> {
    key: K,
    reads: HashSet<K>,
    feeds: HashSet<K>,
}

pub(crate) struct Graph<K, J> {
    running: HashMap<J, Running<K>>,
    reads: HashMap<K, HashSet<K>>,
    feeds: HashMap<K, HashSet<K>>,
    reverse: HashMap<K, HashSet<K>>,
    revision: u64,
}

impl<K, J> Default for Graph<K, J> {
    fn default() -> Self {
        Self {
            running: HashMap::default(),
            reads: HashMap::default(),
            feeds: HashMap::default(),
            reverse: HashMap::default(),
            revision: 0,
        }
    }
}

// These sets represent a conservative mathematical closure; traversal and update ordering cannot
// change its membership or the number of edges. The external API documents that order is absent.
#[cfg_attr(not(test), expect(rustc::potential_query_instability))]
impl<K: Copy + Eq + Hash, J: Copy + Eq + Hash> Graph<K, J> {
    pub(crate) fn running_key(&self, job: J) -> Option<K> {
        self.running.get(&job).map(|running| running.key)
    }

    pub(crate) fn begin(&mut self, job: J, key: K) {
        assert!(
            self.running
                .insert(job, Running { key, reads: HashSet::default(), feeds: HashSet::default() })
                .is_none(),
            "resident query job already active"
        );
    }

    pub(crate) fn read(&mut self, job: J, key: K) {
        if let Some(running) = self.running.get_mut(&job) {
            if running.key != key {
                running.reads.insert(key);
            }
        }
    }

    pub(crate) fn feed(&mut self, job: J, key: K) {
        if let Some(running) = self.running.get_mut(&job) {
            if running.key != key {
                running.feeds.insert(key);
            }
        }
    }

    /// Preserve reads of a memoized computation whose lifetime is independent of a query job.
    /// Multiple computations of the same key conservatively retain their combined read sets.
    pub(crate) fn record_external(&mut self, consumer: K, inputs: impl IntoIterator<Item = K>) {
        for input in inputs {
            if consumer != input && self.reads.entry(consumer).or_default().insert(input) {
                self.reverse.entry(input).or_default().insert(consumer);
            }
        }
    }

    pub(crate) fn end(&mut self, job: J, commit: bool) {
        let running = self.running.remove(&job).expect("resident query job not active");
        if !commit {
            return;
        }
        let old_reads = self.reads.insert(running.key, running.reads);
        let old_feeds = self.feeds.insert(running.key, running.feeds);
        if let Some(old_reads) = old_reads {
            for key in old_reads {
                self.refresh(key, running.key);
            }
        }
        if let Some(old_feeds) = old_feeds {
            for key in old_feeds {
                self.refresh(running.key, key);
            }
        }
        // Avoid cloning the sets to appease the borrow checker; the forward graph is already
        // installed, so newly appearing edges can simply be added to the separate reverse graph.
        for &key in &self.reads[&running.key] {
            self.reverse.entry(key).or_default().insert(running.key);
        }
        for &key in &self.feeds[&running.key] {
            self.reverse.entry(running.key).or_default().insert(key);
        }
    }

    fn refresh(&mut self, input: K, consumer: K) {
        let read = self.reads.get(&consumer).is_some_and(|reads| reads.contains(&input));
        let fed = self.feeds.get(&input).is_some_and(|feeds| feeds.contains(&consumer));
        if !read && !fed {
            if let Some(consumers) = self.reverse.get_mut(&input) {
                consumers.remove(&consumer);
                if consumers.is_empty() {
                    self.reverse.remove(&input);
                }
            }
        }
    }

    pub(crate) fn closure(&self, inputs: impl IntoIterator<Item = K>) -> HashSet<K> {
        assert!(self.running.is_empty(), "resident graph can only be inspected between query jobs");
        let mut found: HashSet<K> = inputs.into_iter().collect();
        let mut pending: Vec<K> = found.iter().copied().collect();
        while let Some(input) = pending.pop() {
            if let Some(consumers) = self.reverse.get(&input) {
                for &consumer in consumers {
                    if found.insert(consumer) {
                        pending.push(consumer);
                    }
                }
            }
        }
        found
    }

    pub(crate) fn advance(&mut self, inputs: impl IntoIterator<Item = K>) -> (u64, HashSet<K>) {
        let closure = self.closure(inputs);
        self.revision = self.revision.checked_add(1).expect("resident graph revision overflow");
        (self.revision, closure)
    }

    pub(crate) fn dependencies(&self, key: K) -> HashSet<K> {
        assert!(self.running.is_empty(), "resident graph can only be inspected between query jobs");
        self.reverse
            .iter()
            .filter_map(|(&input, consumers)| consumers.contains(&key).then_some(input))
            .collect()
    }

    pub(crate) fn stats(&self) -> (u64, usize, usize) {
        assert!(self.running.is_empty(), "resident graph can only be inspected between query jobs");
        let edges = self.reverse.values().map(HashSet::len).sum();
        (self.revision, self.reads.len(), edges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(values: &[char]) -> HashSet<char> {
        values.iter().copied().collect()
    }

    fn record(graph: &mut Graph<char, u8>, job: u8, key: char, reads: &[char], feeds: &[char]) {
        graph.begin(job, key);
        for &read in reads {
            graph.read(job, read);
        }
        for &fed in feeds {
            graph.feed(job, fed);
        }
        graph.end(job, true);
    }

    #[test]
    fn transitive_diamond_preserves_unrelated_queries() {
        let mut graph = Graph::default();
        record(&mut graph, 1, 'a', &['b', 'c', 'b'], &[]);
        record(&mut graph, 2, 'b', &['d'], &[]);
        record(&mut graph, 3, 'c', &['d'], &[]);
        record(&mut graph, 4, 'z', &['y'], &[]);
        assert_eq!(graph.closure(['d']), set(&['a', 'b', 'c', 'd']));
        assert_eq!(graph.dependencies('a'), set(&['b', 'c']));
        assert_eq!(graph.stats(), (0, 4, 5));
    }

    #[test]
    fn recomputation_replaces_edges_and_survives_abandoned_jobs() {
        let mut graph = Graph::default();
        record(&mut graph, 1, 'a', &['b'], &[]);
        assert_eq!(graph.advance(['b']), (1, set(&['a', 'b'])));
        graph.begin(2, 'a');
        graph.read(2, 'c');
        graph.end(2, false);
        assert_eq!(graph.closure(['b']), set(&['a', 'b']));
        assert_eq!(graph.closure(['c']), set(&['c']));
        record(&mut graph, 3, 'a', &['c'], &[]);
        assert_eq!(graph.advance(['b']), (2, set(&['b'])));
        assert_eq!(graph.advance(['c']), (3, set(&['a', 'c'])));
    }

    #[test]
    fn reads_and_feeds_contribute_independently_and_cycles_terminate() {
        let mut graph = Graph::default();
        record(&mut graph, 1, 'a', &['b'], &[]);
        record(&mut graph, 2, 'b', &['a'], &['a']);
        assert_eq!(graph.closure(['b']), set(&['a', 'b']));
        record(&mut graph, 3, 'a', &[], &[]);
        assert_eq!(graph.closure(['b']), set(&['a', 'b']));
        record(&mut graph, 4, 'b', &['a'], &[]);
        assert_eq!(graph.closure(['b']), set(&['b']));
        assert_eq!(graph.closure(['a']), set(&['a', 'b']));
    }

    #[test]
    fn overlapping_jobs_record_their_own_reads() {
        let mut graph = Graph::default();
        graph.begin(1, 'a');
        graph.begin(2, 'b');
        assert_eq!(graph.running_key(1), Some('a'));
        assert_eq!(graph.running_key(2), Some('b'));
        graph.read(1, 'x');
        graph.read(2, 'y');
        graph.end(1, true);
        graph.end(2, true);
        assert_eq!(graph.running_key(1), None);
        assert_eq!(graph.closure(['x']), set(&['a', 'x']));
        assert_eq!(graph.closure(['y']), set(&['b', 'y']));
    }

    #[test]
    fn memoized_tasks_preserve_nested_dependencies_for_later_cache_hits() {
        let mut graph = Graph::default();
        record(&mut graph, 1, 'a', &['s'], &[]);
        graph.record_external('s', ['t']);
        graph.record_external('t', ['x']);
        record(&mut graph, 2, 'b', &['s'], &[]);
        record(&mut graph, 3, 'z', &['y'], &[]);
        assert_eq!(graph.closure(['x']), set(&['a', 'b', 's', 't', 'x']));
        graph.record_external('t', ['t', 'y']);
        assert_eq!(graph.closure(['x']), set(&['a', 'b', 's', 't', 'x']));
        assert_eq!(graph.closure(['y']), set(&['a', 'b', 's', 't', 'y', 'z']));
    }

    #[test]
    fn invalidated_borrower_restores_consumed_prerequisites_and_their_other_readers() {
        let mut graph = Graph::default();
        // Both consumers can read `p`, whose destructive value in turn reads `t`. The first
        // consumer also reads an independent input that can change after `p` and `t` were stolen.
        record(&mut graph, 1, 'a', &['p', 'x'], &['p']);
        record(&mut graph, 2, 'b', &['p'], &['p']);
        record(&mut graph, 3, 'p', &['t'], &['t']);
        record(&mut graph, 4, 't', &['v'], &[]);
        record(&mut graph, 5, 'z', &['y'], &[]);
        assert_eq!(graph.closure(['x']), set(&['a', 'b', 'p', 't', 'x']));
        assert_eq!(graph.closure(['v']), set(&['a', 'b', 'p', 't', 'v']));
        assert_eq!(graph.closure(['y']), set(&['y', 'z']));
    }

    #[test]
    #[should_panic(expected = "between query jobs")]
    fn revisions_are_quiescent() {
        let mut graph = Graph::default();
        graph.begin(1, 'a');
        graph.advance(['a']);
    }
}
