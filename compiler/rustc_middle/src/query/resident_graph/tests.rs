use super::*;

fn set(values: &[char]) -> HashSet<char> {
    values.iter().copied().collect()
}

fn record(graph: &mut Graph<char, u8>, job: u8, key: char, reads: &[char], feeds: &[char]) {
    graph.begin(job, key);
    for &read in reads {
        graph.read(Some(job), read);
    }
    for &fed in feeds {
        graph.feed(job, fed);
    }
    graph.end(job, true);
}

#[test]
fn transitive_diamond_preserves_unrelated_queries() {
    let mut graph = Graph::default();
    record(&mut graph, 1, 'a', &['b', 'c', 'b', 'c', 'b', 'c'], &[]);
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
    graph.read(Some(2), 'c');
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
    record(&mut graph, 2, 'b', &['a'], &['a', 'a', 'a', 'a', 'a']);
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
    assert_eq!(graph.revision(), 0);
    assert_eq!(graph.running_key(1), Some('a'));
    assert_eq!(graph.running_key(2), Some('b'));
    graph.read(Some(1), 'x');
    graph.read(Some(2), 'y');
    graph.end(1, true);
    graph.end(2, true);
    assert_eq!(graph.running_key(1), None);
    assert_eq!(graph.closure(['x']), set(&['a', 'x']));
    assert_eq!(graph.closure(['y']), set(&['b', 'y']));
    graph.advance(['x']);
    graph.begin(3, 'a');
    assert_eq!(graph.revision(), 1);
    graph.end(3, true);
}

#[test]
fn cached_node_word_preserves_first_and_last_indices_and_feed_flags() {
    assert_eq!(std::mem::size_of::<Option<CachedNode>>(), 4);
    for key in [0, 1, CachedNode::FEED - 2] {
        for feed in [false, true] {
            let node = CachedNode::new(key, feed);
            assert_eq!(node.key(), key);
            assert_eq!(node.feed_on_read(), feed);
        }
    }
    assert!(std::panic::catch_unwind(|| CachedNode::new(CachedNode::FEED - 1, false)).is_err());
}

#[test]
fn cached_indices_preserve_reads_feeds_and_recomputed_keys() {
    let mut graph = Graph::default();
    graph.register_cached(4, 'p', true);
    assert_eq!(graph.cached_key(3), None);
    graph.begin(1, 'a');
    assert_eq!(graph.read_cached(Some(1), 3, true), None);
    assert!(graph.read_cached(Some(1), 4, true).is_some());
    graph.end(1, true);
    record(&mut graph, 2, 'p', &['x'], &[]);
    assert_eq!(graph.closure(['x']), set(&['x', 'p', 'a']));
    assert_eq!(graph.closure(['a']), set(&['a', 'p']));
    graph.advance(['x']);
    graph.register_cached(9, 'p', true);
    graph.begin(3, 'b');
    assert!(graph.read_cached(Some(3), 9, false).is_some());
    graph.end(3, true);
    assert_eq!(graph.closure(['x']), set(&['x', 'p', 'a', 'b']));
    assert_eq!(graph.closure(['b']), set(&['b']));
    assert_eq!(graph.cached_key(4), graph.cached_key(9));
    graph.register_cached(10, 'z', false);
    assert_eq!(graph.cached_key(10), Some('z'));
    assert_eq!(graph.closure(['z']), set(&['z']));
    graph.begin(4, 'c');
    let z = graph.read_cached(Some(4), 10, true).unwrap();
    graph.end(4, true);
    assert_eq!(graph.closure(['c']), set(&['c']));
    assert_eq!(graph.closure(['z']), set(&['c', 'z']));
    // An unparented cache hit can still be an input to an external memoized task.
    assert_eq!(graph.read_cached(None, 10, false), Some(z));
    let task = graph.intern_key('t');
    graph.record_external_nodes(task, [z]);
    assert_eq!(graph.closure(['z']), set(&['c', 't', 'z']));
}

#[test]
fn cached_reads_preserve_value_feeds_and_external_task_inputs() {
    let mut graph = Graph::default();
    let mut cache = CachedReads::new();
    graph.register_cached(4, 'p', true);
    record(&mut graph, 1, 'p', &['x'], &[]);
    graph.begin(2, 'a');
    let node = cache.read(1, 2, 4, false, || graph.read_cached(Some(2), 4, false)).unwrap();
    assert_eq!(cache.read(1, 2, 4, false, || panic!("duplicate ensure")), Some(node));
    // A value read after an ensure must add the edge that restores a consumed prerequisite.
    assert_eq!(cache.read(1, 2, 4, true, || graph.read_cached(Some(2), 4, true)), Some(node));
    for task in ['s', 't'] {
        let input = cache.read(1, 2, 4, true, || panic!("duplicate value read")).unwrap();
        let task = graph.intern_key(task);
        graph.record_external_nodes(task, [input]);
    }
    assert_eq!(cache.read(1, 2, 4, false, || panic!("ensure after value")), Some(node));
    graph.end(2, true);
    assert_eq!(graph.closure(['a']), set(&['a', 'p', 's', 't']));
    assert_eq!(graph.closure(['x']), set(&['x', 'p', 'a', 's', 't']));

    // A new job must replace the old read/feed set even for the same query and cache index.
    graph.begin(3, 'a');
    cache.read(1, 3, 4, false, || graph.read_cached(Some(3), 4, false)).unwrap();
    graph.end(3, true);
    assert_eq!(graph.closure(['a']), set(&['a']));
    assert_eq!(graph.closure(['x']), set(&['x', 'p', 'a', 's', 't']));
}

#[test]
fn cached_reads_separate_jobs_sessions_collisions_and_unregistered_indices() {
    let mut cache = CachedReads::new();
    assert_eq!(cache.read(1, 1, 7, true, || None), None);
    assert_eq!(cache.read(1, 1, 7, true, || Some(10)), Some(10));
    assert_eq!(cache.read(1, 1, 7, true, || panic!("duplicate read")), Some(10));
    let collision = 7 + cache.entries.len();
    assert_eq!(cache.read(1, 1, collision, true, || Some(20)), Some(20));
    // Eviction changes cost, never whether the graph records the dependency.
    assert_eq!(cache.read(1, 1, 7, true, || Some(11)), Some(11));
    assert_eq!(cache.read(1, 2, 7, true, || Some(12)), Some(12));
    assert_eq!(cache.read(2, 2, 7, true, || Some(13)), Some(13));
    assert_eq!(cache.read(2, 2, 7, true, || panic!("duplicate read")), Some(13));
}

#[test]
fn interned_keys_without_committed_edges_remain_distinct_from_empty_queries() {
    let mut graph = Graph::default();
    graph.register_cached(2, 'u', false);
    graph.begin(0, 'a');
    graph.read(Some(0), 'b');
    graph.end(0, false);
    graph.record_external('s', ['s']);
    assert_eq!(graph.stats(), (0, 0, 0));
    assert_eq!(graph.closure(['u']), set(&['u']));
    assert_eq!(graph.dependencies('b'), set(&[]));
    graph.record_external('s', ['x', 'x']);
    assert_eq!(graph.stats(), (0, 1, 1));
    record(&mut graph, 1, 'a', &['x', 'x'], &[]);
    assert_eq!(graph.stats(), (0, 2, 2));
    record(&mut graph, 2, 'a', &[], &[]);
    assert_eq!(graph.stats(), (0, 2, 1));
    assert_eq!(graph.closure(['x']), set(&['s', 'x']));
    record(&mut graph, 3, 'u', &[], &[]);
    assert_eq!(graph.stats(), (0, 3, 1));
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
fn ensuring_linear_queries_does_not_restore_all_of_an_orchestrators_inputs() {
    let mut graph = Graph::default();
    // The global orchestrator `o` ensures `p` and `q` without accessing their one-use values.
    // The actual borrower `a` reads `p`'s value and separately depends on changed input `x`.
    record(&mut graph, 1, 'o', &['p', 'q'], &[]);
    record(&mut graph, 2, 'a', &['p', 'x'], &['p']);
    record(&mut graph, 3, 'b', &['q'], &['q']);
    assert_eq!(graph.closure(['x']), set(&['a', 'o', 'p', 'x']));
    assert_eq!(graph.closure(['q']), set(&['b', 'o', 'q']));
}

#[test]
#[should_panic(expected = "between query jobs")]
fn revisions_are_quiescent() {
    let mut graph = Graph::default();
    graph.begin(1, 'a');
    graph.advance(['a']);
}

#[test]
fn backdating_preserves_alternate_dirty_paths_and_future_dependencies() {
    let mut graph = Graph::default();
    // `s` is revalidated unchanged. Its exclusive reader `a` stays cached, but `b`
    // independently reads changed input `x`, so both `b` and its reader `c` remain dirty.
    record(&mut graph, 1, 's', &['x'], &[]);
    record(&mut graph, 2, 'a', &['s'], &[]);
    record(&mut graph, 3, 'b', &['s', 'x'], &[]);
    record(&mut graph, 4, 'c', &['b'], &[]);
    assert_eq!(graph.advance_with_backdated(['x'], &set(&['s'])), (1, set(&['x', 'b', 'c'])));
    // Revalidation replaces dependencies normally; backdating applies only to one revision.
    record(&mut graph, 5, 's', &['y'], &[]);
    assert_eq!(graph.advance(['y']), (2, set(&['y', 's', 'a', 'b', 'c'])));
}

#[test]
fn causal_paths_respect_backdating_and_distinguish_reads_from_feeds() {
    let mut graph = Graph::default();
    record(&mut graph, 1, 's', &['x', 't'], &['p']);
    record(&mut graph, 2, 't', &['p', 'b'], &[]);
    record(&mut graph, 3, 'b', &['x'], &[]);
    let before = graph.stats();
    assert_eq!(
        graph.shortest_path(['x'], &set(&['b']), |key| key == 't'),
        Some(vec![('x', false, false), ('s', true, false), ('p', false, true), ('t', true, false)])
    );
    assert_eq!(
        graph.shortest_path(['x'], &set(&[]), |key| key == 't'),
        Some(vec![('x', false, false), ('b', true, false), ('t', true, false)])
    );
    assert_eq!(graph.shortest_path(['x'], &set(&['s', 'b']), |key| key == 't'), None);
    assert_eq!(graph.stats(), before);
    record(&mut graph, 4, 'p', &['s'], &[]);
    assert_eq!(
        graph.shortest_path(['s'], &set(&[]), |key| key == 'p'),
        Some(vec![('s', false, false), ('p', true, true)])
    );
}

#[test]
fn repeated_cached_ensure_then_value_read_preserves_consumed_prerequisites() {
    let mut graph = Graph::default();
    graph.register_cached(0, 'p', true);
    record(&mut graph, 1, 'p', &['x', 'x'], &[]);
    graph.begin(2, 'a');
    for _ in 0..3 {
        graph.read_cached(Some(2), 0, false).unwrap();
    }
    for _ in 0..3 {
        graph.read_cached(Some(2), 0, true).unwrap();
    }
    graph.end(2, true);
    assert_eq!(graph.closure(['x']), set(&['x', 'p', 'a']));
    assert_eq!(graph.closure(['a']), set(&['a', 'p']));
    assert_eq!(graph.dependencies('a'), set(&['p']));
    assert_eq!(graph.stats(), (0, 2, 3));
}

#[test]
fn repeated_reads_preserve_nested_jobs_and_error_repair_dependencies() {
    let mut graph = Graph::default();
    graph.register_cached(0, 'x', false);
    graph.begin(1, 'a');
    graph.read(Some(1), 'x');
    graph.begin(2, 'b');
    for job in [2, 1, 2, 1] {
        graph.read_cached(Some(job), 0, true).unwrap();
    }
    graph.end(2, true);
    graph.end(1, true);
    record(&mut graph, 3, 'd', &['a', 'a'], &[]);
    record(&mut graph, 4, 'z', &['y'], &[]);
    assert_eq!(graph.advance(['x']), (1, set(&['x', 'a', 'b', 'd'])));
    // A completed error-producing query still commits its input dependencies.
    record(&mut graph, 5, 'a', &['e', 'e'], &[]);
    assert_eq!(graph.closure(['x']), set(&['x', 'b']));
    assert_eq!(graph.advance(['e']), (2, set(&['e', 'a', 'd'])));
    // Repair replaces the diagnostic dependency with the ordinary input again.
    record(&mut graph, 6, 'a', &['x', 'x'], &[]);
    assert_eq!(graph.closure(['e']), set(&['e']));
    assert_eq!(graph.closure(['x']), set(&['x', 'a', 'b', 'd']));
    assert_eq!(graph.closure(['y']), set(&['y', 'z']));
}
