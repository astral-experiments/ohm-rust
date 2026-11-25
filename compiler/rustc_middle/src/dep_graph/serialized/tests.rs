use super::*;

fn node(kind: DepKind, first: u64, second: u64) -> DepNode {
    DepNode { kind, key_fingerprint: Fingerprint::new(first, second).into() }
}

#[test]
fn row_layout_matches_every_inline_header_and_spill() {
    let n = node(DepKind::CompileMonoItem, 1, 2);
    for width in 1..=DEP_NODE_SIZE {
        for count in 0..=SerializedNodeHeader::MAX_INLINE_LEN + 1 {
            let header = SerializedNodeHeader::new(
                &n,
                DepNodeIndex::from_u32(4),
                Fingerprint::ZERO,
                1 << (8 * (width - 1)),
                count,
            );
            let layout = header.layout();
            assert_eq!(usize::from(layout.width), header.bytes_per_index());
            if let Some(len) = header.len() {
                assert_eq!(layout.num_edges, len);
                assert_eq!(
                    layout.stride as usize,
                    size_of::<SerializedNodeHeader>() + count * width
                );
            } else {
                assert_eq!(layout.stride, 0);
            }
        }
    }
}

#[test]
fn edge_targets_stop_at_each_list_boundary() {
    let mut graph = SerializedDepGraph::default();
    let mut bytes = Vec::new();
    let values = [0x0403_0201u32, 0x0807_0605];
    for (width, count) in [(1, 0), (1, 2), (2, 1), (3, 1), (4, 1), (1, 1), (1, 0), (3, 0), (3, 2)] {
        graph.row_index.push(RowIndex {
            node: node(DepKind::Null, 0, 0),
            edges_start: bytes.len(),
            layout: RowLayout { width: width as u8, num_edges: count as u32, ..Default::default() },
            next: None,
        });
        for value in &values[..count] {
            bytes.extend_from_slice(&value.to_le_bytes()[..width]);
        }
    }
    bytes.extend_from_slice(&[0xff; DEP_NODE_PAD]);
    graph.rows = Some(slice_owned(bytes, |data| &data[..]));

    let expected: &[&[u32]] = &[
        &[],
        &[1, 5],
        &[0x0201],
        &[0x03_0201],
        &[0x0403_0201],
        &[1],
        &[],
        &[],
        &[0x03_0201, 0x07_0605],
    ];
    for (source, expected) in graph.row_index.indices().zip(expected) {
        assert_eq!(
            graph.edge_targets_from(source).map(|index| index.as_u32()).collect::<Vec<_>>(),
            *expected
        );
        let edges: Vec<_> = expected.iter().map(|&i| DepNodeIndex::from_u32(i)).collect();
        assert_eq!(graph.has_same_short_edges(source, &edges), edges.len() <= 1);
        assert_eq!(graph.has_same_short_edges(source, &[]), edges.is_empty());
        assert!(!graph.has_same_short_edges(source, &[DepNodeIndex::from_u32(0)]));
        let mut remaining = expected.iter();
        assert_eq!(
            graph.try_for_each_edge(source, |i| {
                assert_eq!(remaining.next(), Some(&i.as_u32()));
                Some(())
            }),
            Some(())
        );
        assert!(remaining.next().is_none());
        if let Some(&first) = expected.first() {
            let mut stop_at = Some(first);
            assert_eq!(
                graph.try_for_each_edge(source, |i| {
                    assert_eq!(stop_at.take(), Some(i.as_u32()));
                    None
                }),
                None
            );
        }
    }
}

fn graph(nodes: Vec<DepNode>) -> SerializedDepGraph {
    let live_node_count = nodes.iter().filter(|node| node.kind != DepKind::Null).count();
    let mut row_index: IndexVec<SerializedDepNodeIndex, RowIndex> =
        nodes.into_iter().map(|node| RowIndex { node, ..RowIndex::empty() }).collect();
    let mut buckets = vec![None; live_node_count.next_power_of_two()];
    // Chunks can arrive out of order even when the graph is dense.
    for index in row_index.indices().rev() {
        if row_index[index].node.kind != DepKind::Null {
            link_row(&mut row_index, &mut buckets, index);
        }
    }
    SerializedDepGraph { row_index, buckets, live_node_count, ..Default::default() }
}

#[test]
fn reverse_index_checks_full_fingerprints_and_kind_with_sparse_indices() {
    let first = node(DepKind::CompileCodegenUnit, 1, 2);
    let collision = node(DepKind::CompileCodegenUnit, 2, 1);
    let other_kind = node(DepKind::CompileMonoItem, 1, 2);
    let null = node(DepKind::Null, 0, 0);
    assert_eq!(
        fingerprint_hash(first.key_fingerprint),
        fingerprint_hash(collision.key_fingerprint)
    );
    let graph = graph(vec![null, first, null, other_kind, collision]);
    assert_eq!(graph.node_to_index_opt(&first), Some(SerializedDepNodeIndex::from_u32(1)));
    for (node, index) in [(first, 1), (collision, 4), (other_kind, 3)] {
        assert_eq!(graph.node_to_index_opt(&node), Some(SerializedDepNodeIndex::from_u32(index)));
    }
    assert_eq!(graph.node_to_index_opt(&node(DepKind::CompileCodegenUnit, 3, 0)), None);
    assert_eq!(graph.node_to_index_opt(&node(DepKind::SideEffect, 1, 2)), None);
    assert_eq!(graph.node_to_index_opt(&null), None);
    assert_eq!(SerializedDepGraph::default().node_to_index_opt(&first), None);
}

#[test]
fn reverse_index_side_effect_duplicates_use_the_last_index() {
    let duplicate = node(DepKind::SideEffect, 1, 2);
    let collision = node(DepKind::SideEffect, 2, 1);
    let mut graph = graph(vec![duplicate, collision, node(DepKind::Null, 0, 0), duplicate]);
    assert_eq!(graph.node_to_index_opt(&duplicate), Some(SerializedDepNodeIndex::from_u32(3)));
    assert_eq!(graph.node_to_index_opt(&collision), Some(SerializedDepNodeIndex::from_u32(1)));
    // File order was 3, 1, 0. Slot 0 left slot 3 behind the collision.
    // Slot 4 must now replace it without dropping the collision.
    let higher = graph.row_index.push(RowIndex { node: duplicate, ..RowIndex::empty() });
    graph.live_node_count += 1;
    link_row(&mut graph.row_index, &mut graph.buckets, higher);
    assert_eq!(graph.node_to_index_opt(&duplicate), Some(higher));
    assert_eq!(graph.node_to_index_opt(&collision), Some(SerializedDepNodeIndex::from_u32(1)));
}

#[test]
#[should_panic(expected = "does not have an unique index")]
fn reverse_index_rejects_other_duplicate_nodes() {
    let duplicate = node(DepKind::CompileCodegenUnit, 1, 2);
    graph(vec![duplicate, duplicate]).node_to_index_opt(&duplicate);
}

#[test]
fn check_index_reads_requested_nodes_with_sparse_indices_and_spilled_edges() {
    let checked = node(DepKind::typeck_root, 1, 2);
    let borrowed = node(DepKind::mir_borrowck, 2, 1);
    let mut file = MemEncoder::new();
    file.emit_raw_bytes(&[0; 8]);
    let mut seek = SeekIndexBuilder::default();
    let dependencies: Vec<_> = (10..28).chain([300]).map(DepNodeIndex::from_u32).collect();
    let mut count = 0u64;
    // Chunks can arrive out of allocation order from different compiler threads.
    for indices in [vec![300, 500], (0..80).collect()] {
        let mut chunk = MemEncoder::new();
        let mut offsets = Vec::new();
        let mut roots = Vec::new();
        for raw in indices {
            let index = DepNodeIndex::from_u32(raw);
            let node = match raw {
                3 => checked,
                500 => borrowed,
                _ => node(DepKind::CompileCodegenUnit, raw.into(), 0),
            };
            offsets.push((index, chunk.data.len()));
            if matches!(raw, 3 | 500) {
                roots.push((node, index));
            }
            NodeInfo {
                node,
                value_fingerprint: Fingerprint::new(u64::from(raw), 99),
                edges: if raw == 3 { &dependencies } else { &[] },
            }
            .encode(&mut chunk, index);
            count += 1;
        }
        let slots = offsets.last().unwrap().0.as_usize() + 1;
        seek.record_chunk(file.data.len(), slots, &mut offsets, &mut roots);
        file.emit_raw_bytes(&chunk.data);
    }
    // An old root can keep its slot while its bytes move past the
    // newly written rows. Slots already written by workers must survive.
    let retained = node(DepKind::typeck_root, 5, 7);
    let retained_index = DepNodeIndex::from_u32(173);
    let offset = file.data.len();
    NodeInfo { node: retained, value_fingerprint: Fingerprint::ZERO, edges: &dependencies }
        .encode(&mut file, retained_index);
    count += 1;
    seek.record_retained(
        (0..400usize).map(|slot| (slot == retained_index.as_usize()).then_some(offset)),
    );
    let start = file.data.len();
    let node_max = 512;
    let mut old = vec![node(DepKind::Null, 0, 0); 400];
    old[retained_index.as_usize()] = retained;
    let mut previous = graph(old);
    let flags = IndexVec::from_fn_n(
        |i: SerializedDepNodeIndex| AtomicBool::new(i.as_u32() == retained_index.as_u32()),
        400,
    );
    previous.sorted_roots = vec![retained_index.as_u32().to_le_bytes()];
    seek.encode(&mut file, start, node_max, &previous, &flags, 3);
    for value in [u64::from(node_max), count, 2 * dependencies.len() as u64] {
        file.emit_raw_bytes(&value.to_le_bytes());
    }
    file.emit_raw_bytes(rustc_serialize::opaque::MAGIC_END_BYTES);

    let graph = IndexedDepGraph::new(&file.data, 8).unwrap();
    assert_eq!(graph.node_reads(), 0);
    let index = graph.node_to_index_opt(&checked).unwrap();
    assert_eq!(index.as_u32(), 3);
    let entry = graph.node(index).unwrap();
    assert_eq!(entry.fingerprint, Fingerprint::new(3, 99));
    assert_eq!(
        entry.edges().map(|index| index.as_u32()).collect::<Vec<_>>(),
        dependencies.iter().map(|index| index.as_u32()).collect::<Vec<_>>()
    );
    assert_eq!(graph.node_to_index_opt(&borrowed).unwrap().as_u32(), 500);
    let moved = graph.node_to_index_opt(&retained).unwrap();
    assert_eq!(moved.as_u32(), retained_index.as_u32());
    assert_eq!(graph.node(moved).unwrap().edges().count(), dependencies.len());
    assert!(graph.node_to_index_opt(&node(DepKind::typeck_root, 2, 1)).is_none());
    assert!(graph.node(SerializedDepNodeIndex::from_u32(99)).is_none());
    assert!(graph.node(SerializedDepNodeIndex::from_u32(501)).is_none());
    assert!(graph.node(SerializedDepNodeIndex::from_u32(node_max - 1)).is_none());
    assert!(graph.node(SerializedDepNodeIndex::from_u32(node_max)).is_none());
    assert!(graph.node_reads() < graph.node_count() / 4);

    // Old graphs and incomplete seek directories conservatively have no index.
    let mut truncated = file.data.clone();
    truncated.remove(start);
    assert!(IndexedDepGraph::new(&truncated, 8).is_none());
    let mut legacy = file.data[..start].to_vec();
    legacy.extend_from_slice(
        &file.data[file.data.len() - 24 - rustc_serialize::opaque::MAGIC_END_BYTES.len()..],
    );
    assert!(IndexedDepGraph::new(&legacy, 8).is_none());
}

#[test]
fn retained_roots_match_reconstruction_even_with_an_invalid_directory() {
    let root = |n| node(DepKind::typeck_root, n, 2);
    let mut previous = graph(vec![root(1), node(DepKind::Red, 3, 4), root(3), root(4)]);
    let flags =
        IndexVec::from_fn_n(|i: SerializedDepNodeIndex| AtomicBool::new(i.as_u32() != 3), 4);
    let encode = |graph: &SerializedDepGraph| {
        let mut seek = SeekIndexBuilder::default();
        let mut additions = vec![(root(2), DepNodeIndex::from_u32(4))];
        seek.record_chunk(32, 5, &mut Vec::new(), &mut additions);
        let mut file = MemEncoder::new();
        seek.encode(&mut file, 0, 5, graph, &flags, 3);
        file.data
    };
    let expected = encode(&previous);
    for slots in [vec![0, 2, 3], vec![0], vec![2, 0], vec![0, 0], vec![0, 1], vec![u32::MAX]] {
        previous.sorted_roots = slots.into_iter().map(u32::to_le_bytes).collect();
        assert_eq!(encode(&previous), expected);
    }
}
