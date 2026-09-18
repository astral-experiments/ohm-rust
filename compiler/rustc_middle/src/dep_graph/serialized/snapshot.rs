//! An optional shareable layout for one immutable incremental dependency-graph generation.

use std::hash::Hasher;
use std::io::{self, Write};
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use rustc_data_structures::fingerprint::{Fingerprint, PackedFingerprint};
use rustc_data_structures::memmap::Mmap;
use rustc_data_structures::profiling::SelfProfilerRef;
use rustc_data_structures::stable_hash::StableHasher;
use rustc_data_structures::unhash::UnhashSet;
use rustc_index::IndexVec;

use super::{
    DEP_NODE_PAD, DepKind, DepNode, EdgeHeader, LazyKindIndex, LazyNodeIndex, SerializedDepGraph,
    SerializedDepNodeIndex,
};

const MAGIC: &[u8; 8] = b"OHMDFG01";
const HEADER_BYTES: usize = 80;
const NODE_BYTES: usize = 46;
const VALUE_OFFSET: usize = 18;

pub(super) struct MappedDepGraphSnapshot {
    data: Mmap,
    edges: Range<usize>,
}

impl std::fmt::Debug for MappedDepGraphSnapshot {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt.debug_struct("MappedDepGraphSnapshot")
            .field("bytes", &self.data.len())
            .field("edges", &self.edges)
            .finish()
    }
}

impl MappedDepGraphSnapshot {
    #[inline]
    pub(super) fn value_fingerprint(&self, index: SerializedDepNodeIndex) -> Fingerprint {
        let start = HEADER_BYTES + index.index() * NODE_BYTES + VALUE_OFFSET;
        Fingerprint::from_le_bytes(self.data[start..start + 16].try_into().unwrap())
    }

    #[inline]
    pub(super) fn edge_bytes(&self) -> &[u8] {
        &self.data[self.edges.clone()]
    }
}

fn hash_parts(parts: &[&[u8]]) -> Fingerprint {
    let mut hasher = StableHasher::new();
    for part in parts {
        hasher.write(part);
    }
    hasher.finish()
}

fn fixed<const N: usize>(data: &[u8], position: usize) -> Option<[u8; N]> {
    data.get(position..position.checked_add(N)?)?.try_into().ok()
}

fn read_u32(data: &[u8], position: usize) -> Option<u32> {
    Some(u32::from_le_bytes(fixed(data, position)?))
}

fn read_usize(data: &[u8], position: usize) -> Option<usize> {
    usize::try_from(u64::from_le_bytes(fixed(data, position)?)).ok()
}

impl SerializedDepGraph {
    /// Identifies the complete stock graph, including its version and compiler-options header.
    pub fn snapshot_generation(bytes: &[u8]) -> Fingerprint {
        hash_parts(&[bytes])
    }

    /// Opens a flat view when it is intact and belongs to the checked stock generation.
    ///
    /// Stable node identities and the lazily-populated inverse remain process-local. Query-value
    /// fingerprints and all edge bytes stay in the read-only mapping shared through the OS cache.
    pub fn from_snapshot(
        data: Mmap,
        source_generation: Fingerprint,
        profiler: &SelfProfilerRef,
    ) -> Option<Arc<Self>> {
        if data.len() < HEADER_BYTES || data.get(..8)? != MAGIC {
            return None;
        }
        if Fingerprint::from_le_bytes(fixed(&data, 8)?) != source_generation
            || Fingerprint::from_le_bytes(fixed(&data, 24)?)
                != hash_parts(&[data.get(..24)?, data.get(40..)?])
        {
            return None;
        }

        let node_max = read_usize(&data, 40)?;
        let active_count = read_usize(&data, 48)?;
        let edge_len = read_usize(&data, 56)?;
        let session_count = u64::from_le_bytes(fixed(&data, 64)?);
        let kinds_count = read_usize(&data, 72)?;
        if kinds_count != usize::from(DepKind::MAX) + 1
            || active_count > node_max
            || node_max > (SerializedDepNodeIndex::MAX.as_u32() as usize).checked_add(1)?
            || edge_len < DEP_NODE_PAD
        {
            return None;
        }
        let kinds_start = HEADER_BYTES.checked_add(node_max.checked_mul(NODE_BYTES)?)?;
        let indices_start = kinds_start.checked_add(kinds_count.checked_mul(4)?)?;
        let edges_start = indices_start.checked_add(active_count.checked_mul(4)?)?;
        let edges_end = edges_start.checked_add(edge_len)?;
        if edges_end != data.len()
            || data.get(edges_end - DEP_NODE_PAD..edges_end)? != [0; DEP_NODE_PAD]
        {
            return None;
        }

        let mut nodes = IndexVec::with_capacity(node_max);
        let mut edge_list_indices = IndexVec::with_capacity(node_max);
        let mut seen_active = 0;
        for index in 0..node_max {
            let start = HEADER_BYTES + index * NODE_BYTES;
            let kind = u16::from_le_bytes(fixed(&data, start)?);
            if kind > DepKind::MAX {
                return None;
            }
            let kind = DepKind::from_u16(kind);
            if kind != DepKind::Null {
                seen_active += 1;
            }
            nodes.push(DepNode {
                kind,
                key_fingerprint: PackedFingerprint::from(Fingerprint::from_le_bytes(fixed(
                    &data,
                    start + 2,
                )?)),
            });
            let header = EdgeHeader {
                repr: read_usize(&data, start + 34)?,
                num_edges: read_u32(&data, start + 42)?,
            };
            let used = (header.num_edges as usize).checked_mul(header.bytes_per_index())?;
            if header.start().checked_add(used)? > edge_len - DEP_NODE_PAD {
                return None;
            }
            edge_list_indices.push(header);
        }
        if seen_active != active_count {
            return None;
        }

        let edges = data.get(edges_start..edges_end)?;
        for header in &edge_list_indices {
            let mut start = header.start();
            for _ in 0..header.num_edges {
                let target = read_u32(edges, start)? & header.mask();
                if target as usize >= node_max
                    || nodes[SerializedDepNodeIndex::from_u32(target)].kind == DepKind::Null
                {
                    return None;
                }
                start += header.bytes_per_index();
            }
        }

        let mut kinds = Vec::with_capacity(kinds_count);
        let mut nodes_by_kind = Vec::with_capacity(active_count);
        let mut seen = vec![false; node_max];
        for kind in 0..kinds_count {
            let len = read_u32(&data, kinds_start + kind * 4)?;
            if kind == DepKind::Null.as_usize() && len != 0 {
                return None;
            }
            let start = u32::try_from(nodes_by_kind.len()).ok()?;
            let range_end = nodes_by_kind.len().checked_add(len as usize)?;
            if range_end > active_count {
                return None;
            }
            let mut fingerprints = UnhashSet::default();
            while nodes_by_kind.len() < range_end {
                let raw = read_u32(&data, indices_start + nodes_by_kind.len() * 4)?;
                let index = raw as usize;
                if index >= node_max || seen[index] {
                    return None;
                }
                let node = nodes[SerializedDepNodeIndex::from_u32(raw)];
                if node.kind == DepKind::Null
                    || node.kind.as_usize() != kind
                    || (node.kind != DepKind::SideEffect
                        && !fingerprints.insert(node.key_fingerprint))
                {
                    return None;
                }
                seen[index] = true;
                nodes_by_kind.push(Some(SerializedDepNodeIndex::from_u32(raw)));
            }
            kinds.push(LazyKindIndex { start, len, map: OnceLock::new() });
        }
        if nodes_by_kind.len() != active_count {
            return None;
        }

        Some(Arc::new(Self {
            nodes,
            value_fingerprints: IndexVec::new(),
            edge_list_indices,
            edge_list_data: Vec::new(),
            reverse_index: LazyNodeIndex { nodes_by_kind, kinds },
            session_count,
            profiler: Some(profiler.clone()),
            snapshot: Some(MappedDepGraphSnapshot { data, edges: edges_start..edges_end }),
        }))
    }

    /// Publishes the primitive little-endian flat data for this generation to a private writer.
    pub fn write_snapshot(
        &self,
        source_generation: Fingerprint,
        mut output: impl Write,
    ) -> io::Result<()> {
        let node_max = self.nodes.len();
        let active_count = self.reverse_index.nodes_by_kind.len();
        let edges = self.edge_bytes();
        let kinds = &self.reverse_index.kinds;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&source_generation.to_le_bytes());
        bytes.extend_from_slice(&[0; 16]);
        for count in [node_max, active_count, edges.len()] {
            bytes.extend_from_slice(&(count as u64).to_le_bytes());
        }
        bytes.extend_from_slice(&self.session_count.to_le_bytes());
        bytes.extend_from_slice(&(kinds.len() as u64).to_le_bytes());
        debug_assert_eq!(bytes.len(), HEADER_BYTES);

        for (index, node) in self.nodes.iter_enumerated() {
            bytes.extend_from_slice(&node.kind.as_u16().to_le_bytes());
            bytes.extend_from_slice(&Fingerprint::from(node.key_fingerprint).to_le_bytes());
            bytes.extend_from_slice(&self.value_fingerprint_for_index(index).to_le_bytes());
            let edge = self.edge_list_indices[index];
            bytes.extend_from_slice(&(edge.repr as u64).to_le_bytes());
            bytes.extend_from_slice(&edge.num_edges.to_le_bytes());
        }
        for kind in kinds {
            bytes.extend_from_slice(&kind.len.to_le_bytes());
        }
        for index in &self.reverse_index.nodes_by_kind {
            let index = index.expect("decoded generation fills every inverse-index slot");
            bytes.extend_from_slice(&index.as_u32().to_le_bytes());
        }
        bytes.extend_from_slice(edges);
        let checksum = hash_parts(&[&bytes[..24], &bytes[40..]]);
        bytes[24..40].copy_from_slice(&checksum.to_le_bytes());
        output.write_all(&bytes)
    }
}

#[cfg(test)]
mod tests;
