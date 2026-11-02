use super::*;

fn node(kind: DepKind, first: u64, second: u64) -> DepNode {
    DepNode { kind, key_fingerprint: Fingerprint::new(first, second).into() }
}

#[test]
fn edge_targets_stop_at_each_list_boundary() {
    let mut graph = SerializedDepGraph::default();
    let values = [0x0403_0201u32, 0x0807_0605];
    for (width, count) in [(1, 0), (1, 2), (2, 1), (3, 1), (4, 1), (1, 1), (1, 0)] {
        graph.edge_list_indices.push(EdgeHeader {
            repr: (graph.edge_list_data.len() << DEP_NODE_WIDTH_BITS) | (width - 1),
            num_edges: count as u32,
        });
        for value in &values[..count] {
            graph.edge_list_data.extend_from_slice(&value.to_le_bytes()[..width]);
        }
    }
    graph.edge_list_data.extend_from_slice(&[0; DEP_NODE_PAD]);

    let expected: &[&[u32]] = &[&[], &[1, 5], &[0x0201], &[0x03_0201], &[0x0403_0201], &[1], &[]];
    for (source, expected) in graph.edge_list_indices.indices().zip(expected) {
        assert_eq!(
            graph.edge_targets_from(source).map(|index| index.as_u32()).collect::<Vec<_>>(),
            *expected
        );
    }
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
    let node_max = 512;
    seek.encode(&mut file, start, node_max);
    for value in [u64::from(node_max), count, dependencies.len() as u64] {
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

/// Two workers flush in the opposite order to their allocated indices. Both retain unused tails.
fn check_graph(fixed: bool) -> Vec<u8> {
    let mut file = MemEncoder::new();
    file.emit_raw_bytes(&[0; 8]);
    if fixed {
        file.emit_raw_bytes(fixed::MAGIC);
    }
    let mut packed_index = SeekIndexBuilder::default();
    let mut directory = fixed::DirectoryWriter::default();
    let mut counts = vec![0u32; DepKind::NUM_VARIANTS];
    for (first, kinds) in [
        (256u32, [DepKind::mir_borrowck, DepKind::CompileMonoItem]),
        (0, [DepKind::typeck_root, DepKind::CompileCodegenUnit]),
    ] {
        let mut worker = fixed::BlockWriter::default();
        let mut chunk = MemEncoder::new();
        let mut positions = Vec::new();
        let mut roots = Vec::new();
        for (i, kind) in kinds.into_iter().enumerate() {
            let id = first + i as u32;
            let edges = [DepNodeIndex::from_u32(257), DepNodeIndex::from_u32(1)];
            counts[kind.as_usize()] += 1;
            let index = DepNodeIndex::from_u32(id);
            let info = NodeInfo {
                node: node(kind, u64::from(id), 31),
                value_fingerprint: Fingerprint::new(19, u64::from(id)),
                edges: if id == 0 { &edges } else { &[] },
            };
            if fixed {
                worker.push(index, &info);
            } else {
                positions.push((index, chunk.data.len()));
                if matches!(kind, DepKind::mir_borrowck | DepKind::typeck_root) {
                    roots.push((info.node, index));
                }
                info.encode(&mut chunk, index);
            }
        }
        let position = file.data.len();
        if fixed {
            worker.flush(&mut file, position, &mut directory);
        } else {
            packed_index.record_chunk(position, &mut positions, &mut roots);
            file.emit_raw_bytes(&chunk.data);
        }
    }
    let stats = file.data.len();
    for count in counts {
        count.encode(&mut file);
    }
    7u64.encode(&mut file);
    let start = file.data.len();
    if fixed {
        directory.encode(&mut file, stats, start, 259);
    } else {
        packed_index.encode(&mut file, start, 259);
    }
    for n in [259u64, 4, 2] {
        file.emit_raw_bytes(&n.to_le_bytes());
    }
    file.emit_raw_bytes(rustc_serialize::opaque::MAGIC_END_BYTES);
    file.data
}

fn read_graph(data: &[u8]) -> Option<Arc<SerializedDepGraph>> {
    SerializedDepGraph::try_decode(
        &mut MemDecoder::new(data, 8).ok()?,
        &SelfProfilerRef::new(None, None),
    )
}

#[test]
fn indexed_check_reads_both_formats_and_keeps_each_row_boundary() {
    for fixed in [false, true] {
        let data = check_graph(fixed);
        let eager = read_graph(&data).unwrap();
        let seek = IndexedDepGraph::new(&data, 8).unwrap();
        let id = SerializedDepNodeIndex::from_u32;
        assert_eq!(eager.session_count, 7);
        assert_eq!(seek.node_count(), 4);
        assert_eq!(seek.node_reads(), 0);
        for (kind, raw) in [(DepKind::typeck_root, 0), (DepKind::mir_borrowck, 256)] {
            let key = node(kind, raw.into(), 31);
            assert_eq!(eager.node_to_index_opt(&key), Some(id(raw)));
            assert_eq!(seek.node_to_index_opt(&key), Some(id(raw)));
            assert_eq!(
                seek.node(id(raw)).unwrap().fingerprint,
                Fingerprint::new(19, u64::from(raw))
            );
        }
        assert!(seek.node_to_index_opt(&node(DepKind::CompileMonoItem, 257, 31)).is_none());
        assert!(seek.node(id(2)).is_none());
        assert!(seek.node(id(258)).is_none());
        assert!(seek.node(id(259)).is_none());
        assert!(eager.edge_targets_from(id(258)).next().is_none());
        let mut edges = eager.edge_targets_from(id(0));
        assert_eq!(edges.next(), Some(id(257)));
        assert_eq!(edges.clone().collect::<Vec<_>>(), [id(1)]);
        assert_eq!(edges.find(|i| *i == id(1)), Some(id(1)));
        assert_eq!(edges.next(), None);
        assert_eq!(edges.next(), None);
        assert_eq!(seek.node(id(0)).unwrap().edges().collect::<Vec<_>>(), [id(257), id(1)]);
    }
}

#[test]
fn fixed_check_rejects_invalid_records_offsets_edges_counts_and_truncation() {
    let data = check_graph(true);
    let end = data.len() - rustc_serialize::opaque::MAGIC_END_BYTES.len();
    let index = fixed::Directory::new(&data[..end], 8).unwrap();
    let block = index.block_offset(0).unwrap();
    let offsets = block + 16 + 2 * 34;
    let edges = offsets + 3 * 4;
    for (at, bytes) in [
        (block, &1u32.to_le_bytes()[..]),
        (block + 4, &0u32.to_le_bytes()),
        (block + 16, &u16::MAX.to_le_bytes()),
        (block + 16, &DepKind::mir_borrowck.as_u16().to_le_bytes()),
        (offsets + 4, &3u32.to_le_bytes()),
        (edges, &u32::MAX.to_le_bytes()),
        (index.stats, &[0xff; 10]),
        (index.start, &0u64.to_le_bytes()),
        (end - 8, &3u64.to_le_bytes()),
    ] {
        let mut bad = data.clone();
        bad[at..at + bytes.len()].copy_from_slice(bytes);
        assert!(read_graph(&bad).is_none(), "accepted invalid bytes at {at}");
    }
    // A numeric target in an unused worker tail is rejected when its parent is visited.
    // No row can turn that hole green, and unrelated live rows keep their usual unknown color.
    let mut bad = data.clone();
    bad[edges + 4..edges + 8].copy_from_slice(&258u32.to_le_bytes());
    let eager = read_graph(&bad).unwrap();
    let colors = DepNodeColorMap::new(&eager);
    let mut targets = eager.edge_targets_from(SerializedDepNodeIndex::from_u32(0));
    let good = targets.next().unwrap();
    let hole = targets.next().unwrap();
    assert!(matches!(colors.get(good), super::super::graph::DepNodeColor::Unknown));
    assert!(matches!(
        colors.try_set_color(good, DesiredColor::Green { index: DepNodeIndex::from_u32(9) }),
        TrySetColorResult::Success
    ));
    assert!(matches!(colors.get(good), super::super::graph::DepNodeColor::Green(_)));
    assert!(matches!(colors.get(hole), super::super::graph::DepNodeColor::Red));
    assert!(matches!(
        colors.try_set_color(hole, DesiredColor::Green { index: DepNodeIndex::from_u32(10) }),
        TrySetColorResult::AlreadyRed
    ));
    assert!(matches!(
        colors.get(SerializedDepNodeIndex::from_u32(256)),
        super::super::graph::DepNodeColor::Unknown
    ));
    let seek = IndexedDepGraph::new(&bad, 8).unwrap();
    assert!(seek.node(hole).is_none());

    for cut in [16, block + 16, offsets, edges + 1, index.start, end - 1] {
        let mut bad = data[..cut].to_vec();
        bad.extend_from_slice(rustc_serialize::opaque::MAGIC_END_BYTES);
        assert!(read_graph(&bad).is_none(), "accepted truncated graph at {cut}");
    }
}

#[test]
fn fixed_graph_marker_cannot_be_a_packed_node() {
    let mut bytes = [0; 38];
    bytes[..fixed::MAGIC.len()].copy_from_slice(fixed::MAGIC);
    assert_eq!(SerializedNodeHeader { bytes }.node().kind, DepKind::Null);
}
