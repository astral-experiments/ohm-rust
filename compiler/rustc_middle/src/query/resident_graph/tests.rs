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
