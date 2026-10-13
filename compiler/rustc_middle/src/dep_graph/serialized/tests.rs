use super::*;

fn node(kind: DepKind, first: u64, second: u64) -> DepNode {
    DepNode { kind, key_fingerprint: Fingerprint::new(first, second).into() }
}

fn graph(nodes: Vec<DepNode>) -> SerializedDepGraph {
    let nodes = IndexVec::from_raw(nodes);
    let mut nodes_by_kind = Vec::new();
    let kinds = (0..DepKind::NUM_VARIANTS)
        .map(|kind| {
            let start = nodes_by_kind.len();
            nodes_by_kind.extend(nodes.iter_enumerated().filter_map(|(index, node)| {
                (node.kind != DepKind::Null && node.kind.as_usize() == kind).then_some(Some(index))
            }));
            LazyKindIndex {
                start: start as u32,
                len: (nodes_by_kind.len() - start) as u32,
                map: OnceLock::new(),
            }
        })
        .collect();
    SerializedDepGraph {
        nodes,
        live_node_count: nodes_by_kind.len(),
        reverse_index: LazyNodeIndex { nodes_by_kind, kinds },
        ..Default::default()
    }
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
    assert!(graph.reverse_index.kinds.iter().all(|kind| kind.map.get().is_none()));
    assert_eq!(graph.node_to_index_opt(&first), Some(SerializedDepNodeIndex::from_u32(1)));
    assert!(graph.reverse_index.kinds[other_kind.kind.as_usize()].map.get().is_none());
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
    let graph = graph(vec![duplicate, collision, node(DepKind::Null, 0, 0), duplicate]);
    assert_eq!(graph.node_to_index_opt(&duplicate), Some(SerializedDepNodeIndex::from_u32(3)));
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
        seek.record_chunk(file.data.len(), &mut offsets, &mut roots);
        file.emit_raw_bytes(&chunk.data);
    }
    let start = file.data.len();
    seek.encode(&mut file, start, 501);
    for value in [501, count, dependencies.len() as u64] {
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
    assert!(graph.node_to_index_opt(&node(DepKind::typeck_root, 2, 1)).is_none());
    assert!(graph.node(SerializedDepNodeIndex::from_u32(99)).is_none());
    assert!(graph.node(SerializedDepNodeIndex::from_u32(501)).is_none());
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
