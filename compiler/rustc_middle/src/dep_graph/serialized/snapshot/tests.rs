use rustc_data_structures::memmap::MmapMut;

use super::*;

fn make_graph() -> SerializedDepGraph {
    let nodes = IndexVec::from_raw(vec![
        DepNode { kind: DepKind::Null, key_fingerprint: Fingerprint::ZERO.into() },
        DepNode { kind: DepKind::SideEffect, key_fingerprint: Fingerprint::new(1u64, 2u64).into() },
        DepNode { kind: DepKind::SideEffect, key_fingerprint: Fingerprint::new(3u64, 4u64).into() },
    ]);
    let mut offset = 0;
    let kinds = (0..=DepKind::MAX)
        .map(|kind| {
            let len = if kind == DepKind::SideEffect.as_u16() { 2 } else { 0 };
            let start = offset;
            offset += len;
            LazyKindIndex { start, len, map: OnceLock::new() }
        })
        .collect();
    SerializedDepGraph {
        nodes,
        value_fingerprints: IndexVec::from_raw(vec![
            Fingerprint::ZERO,
            Fingerprint::new(5u64, 6u64),
            Fingerprint::new(7u64, 8u64),
        ]),
        edge_list_indices: IndexVec::from_raw(vec![
            EdgeHeader { repr: 0, num_edges: 0 },
            EdgeHeader { repr: 0, num_edges: 1 },
            EdgeHeader { repr: 0, num_edges: 0 },
        ]),
        edge_list_data: vec![2, 0, 0, 0],
        reverse_index: LazyNodeIndex {
            nodes_by_kind: vec![
                Some(SerializedDepNodeIndex::from_u32(1)),
                Some(SerializedDepNodeIndex::from_u32(2)),
            ],
            kinds,
        },
        session_count: 4,
        profiler: None,
        snapshot: None,
    }
}

fn map(bytes: &[u8]) -> Mmap {
    let mut data = MmapMut::map_anon(bytes.len()).unwrap();
    data[..].copy_from_slice(bytes);
    data.make_read_only().unwrap()
}

fn update_checksum(bytes: &mut [u8]) {
    let checksum = hash_parts(&[&bytes[..24], &bytes[40..]]);
    bytes[24..40].copy_from_slice(&checksum.to_le_bytes());
}

#[test]
fn round_trip_and_validate_flat_dependency_graph() {
    let expected = make_graph();
    let source = SerializedDepGraph::snapshot_generation(b"the matching stock graph");
    let profiler = SelfProfilerRef::new(None, None);
    let mut bytes = Vec::new();
    expected.write_snapshot(source, &mut bytes).unwrap();
    let graph = SerializedDepGraph::from_snapshot(map(&bytes), source, &profiler).unwrap();

    assert_eq!(graph.node_count(), 3);
    assert_eq!(graph.session_count(), 4);
    assert!(graph.value_fingerprints.is_empty());
    assert!(graph.edge_list_data.is_empty());
    for raw in 0..3 {
        let index = SerializedDepNodeIndex::from_u32(raw);
        assert_eq!(graph.index_to_node(index), expected.index_to_node(index));
        assert_eq!(
            graph.value_fingerprint_for_index(index),
            expected.value_fingerprint_for_index(index)
        );
        assert_eq!(
            graph.edge_targets_from(index).collect::<Vec<_>>(),
            expected.edge_targets_from(index).collect::<Vec<_>>()
        );
        if raw != 0 {
            assert_eq!(graph.node_to_index_opt(graph.index_to_node(index)), Some(index));
        }
    }

    let other = SerializedDepGraph::snapshot_generation(b"another stock graph");
    assert!(SerializedDepGraph::from_snapshot(map(&bytes), other, &profiler).is_none());
    assert!(SerializedDepGraph::from_snapshot(map(&bytes[..79]), source, &profiler).is_none());
    let mut damaged = bytes.clone();
    damaged[HEADER_BYTES + 5] ^= 1;
    assert!(SerializedDepGraph::from_snapshot(map(&damaged), source, &profiler).is_none());

    for position in [40, 48, 56, 72, HEADER_BYTES, HEADER_BYTES + NODE_BYTES + 34] {
        let mut damaged = bytes.clone();
        damaged[position..position + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        update_checksum(&mut damaged);
        assert!(SerializedDepGraph::from_snapshot(map(&damaged), source, &profiler).is_none());
    }

    let kinds_start = HEADER_BYTES + 3 * NODE_BYTES;
    let indices_start = kinds_start + (usize::from(DepKind::MAX) + 1) * 4;
    let mut damaged = bytes.clone();
    damaged[indices_start + 4..indices_start + 8].copy_from_slice(&1u32.to_le_bytes());
    update_checksum(&mut damaged);
    assert!(SerializedDepGraph::from_snapshot(map(&damaged), source, &profiler).is_none());

    let mut damaged = bytes.clone();
    let null_kind = kinds_start + DepKind::Null.as_usize() * 4;
    let side_effect_kind = kinds_start + DepKind::SideEffect.as_usize() * 4;
    damaged[null_kind..null_kind + 4].copy_from_slice(&1u32.to_le_bytes());
    damaged[side_effect_kind..side_effect_kind + 4].copy_from_slice(&1u32.to_le_bytes());
    damaged[indices_start..indices_start + 4].copy_from_slice(&0u32.to_le_bytes());
    update_checksum(&mut damaged);
    assert!(SerializedDepGraph::from_snapshot(map(&damaged), source, &profiler).is_none());

    for bad_target in [0, 127] {
        let mut damaged = bytes.clone();
        let edge_bytes = indices_start + 2 * 4;
        damaged[edge_bytes] = bad_target;
        update_checksum(&mut damaged);
        assert!(SerializedDepGraph::from_snapshot(map(&damaged), source, &profiler).is_none());
    }

    let mut duplicated = bytes;
    let first_key = HEADER_BYTES + NODE_BYTES + 2;
    let second_key = HEADER_BYTES + NODE_BYTES * 2 + 2;
    duplicated.copy_within(first_key..first_key + 16, second_key);
    update_checksum(&mut duplicated);
    assert!(SerializedDepGraph::from_snapshot(map(&duplicated), source, &profiler).is_some());

    let real_kind = DepKind::CompileCodegenUnit;
    for node in 1..=2 {
        let offset = HEADER_BYTES + node * NODE_BYTES;
        duplicated[offset..offset + 2].copy_from_slice(&real_kind.as_u16().to_le_bytes());
    }
    let real_kind_offset = kinds_start + real_kind.as_usize() * 4;
    duplicated[side_effect_kind..side_effect_kind + 4].copy_from_slice(&0u32.to_le_bytes());
    duplicated[real_kind_offset..real_kind_offset + 4].copy_from_slice(&2u32.to_le_bytes());
    update_checksum(&mut duplicated);
    assert!(SerializedDepGraph::from_snapshot(map(&duplicated), source, &profiler).is_none());
}
