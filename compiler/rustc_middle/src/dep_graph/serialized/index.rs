//! Random access to a check's serialized graph, without materializing its nodes or edges.

use std::cell::Cell;

use rustc_data_structures::fingerprint::Fingerprint;
use rustc_serialize::Decoder;
#[cfg(test)]
use rustc_serialize::Encoder;
use rustc_serialize::opaque::MemDecoder;

#[cfg(test)]
use super::DepNodeIndex;
use super::{
    DepKind, DepNode, SerializedDepGraph, SerializedDepNodeIndex, SerializedNodeHeader, fixed,
};

const FOOTER_BYTES: usize = 24;
const INDEX_TRAILER_BYTES: usize = 24;
const INDEX_MAGIC: u64 = u64::from_le_bytes(*b"OHMIDX01");

pub(super) fn key(node: &DepNode) -> (u16, [u8; 16]) {
    (node.kind.as_u16(), Fingerprint::from(node.key_fingerprint).to_le_bytes())
}

#[cfg(test)]
#[derive(Default)]
pub(super) struct SeekIndexBuilder {
    offsets: Vec<u64>,
    roots: Vec<(DepNode, DepNodeIndex)>,
}

#[cfg(test)]
impl SeekIndexBuilder {
    pub(super) fn record_chunk(
        &mut self,
        start: usize,
        offsets: &mut Vec<(DepNodeIndex, usize)>,
        roots: &mut Vec<(DepNode, DepNodeIndex)>,
    ) {
        for (index, offset) in offsets.drain(..) {
            let index = index.as_u32() as usize;
            if index >= self.offsets.len() {
                self.offsets.resize(index + 1, 0);
            }
            debug_assert_eq!(self.offsets[index], 0);
            self.offsets[index] = (start + offset).try_into().unwrap();
        }
        self.roots.append(roots);
    }

    /// Appends an optional seek directory before the ordinary graph footer.
    pub(super) fn encode(&mut self, encoder: &mut impl Encoder, start: usize, node_max: u32) {
        assert!(self.offsets.len() <= node_max as usize);
        self.offsets.resize(node_max as usize, 0);
        for offset in &self.offsets {
            encoder.emit_raw_bytes(&offset.to_le_bytes());
        }
        self.roots.sort_unstable_by_key(|(node, _)| key(node));
        for (_, index) in &self.roots {
            encoder.emit_raw_bytes(&index.as_u32().to_le_bytes());
        }
        for value in [start as u64, self.roots.len() as u64, INDEX_MAGIC] {
            encoder.emit_raw_bytes(&value.to_le_bytes());
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SeekIndexMetadata {
    pub(super) start: usize,
    roots_start: usize,
    roots_len: usize,
    node_max: usize,
    node_count: usize,
}

impl SeekIndexMetadata {
    pub(super) fn decode(decoder: &mut MemDecoder<'_>) -> Option<Self> {
        let footer = decoder.len().checked_sub(FOOTER_BYTES)?;
        let trailer = footer.checked_sub(INDEX_TRAILER_BYTES)?;
        let values = decoder.with_position(trailer, |d| {
            std::array::from_fn::<_, 6, _>(|_| u64::from_le_bytes(d.read_array()))
        });
        let [start, roots_len, magic, node_max, node_count, _edge_count] = values;
        if magic != INDEX_MAGIC {
            return None;
        }
        let start = usize::try_from(start).ok()?;
        let roots_len = usize::try_from(roots_len).ok()?;
        let node_max = usize::try_from(node_max).ok()?;
        let node_count = usize::try_from(node_count).ok()?;
        let roots_start = start.checked_add(node_max.checked_mul(8)?)?;
        if roots_start.checked_add(roots_len.checked_mul(4)?)? != trailer
            || roots_len > node_count
            || node_count > node_max
            || start
                < decoder
                    .position()
                    .checked_add(node_count.checked_mul(size_of::<SerializedNodeHeader>())?)?
        {
            return None;
        }
        Some(Self { start, roots_start, roots_len, node_max, node_count })
    }
}

pub(crate) struct IndexedDepGraph<'a> {
    data: &'a [u8],
    graph_start: usize,
    index: SeekIndexMetadata,
    fixed: Option<fixed::Directory<'a>>,
    node_reads: Cell<usize>,
}

pub(crate) struct IndexedDepNode<'a> {
    pub(crate) node: DepNode,
    pub(crate) fingerprint: Fingerprint,
    edges: &'a [u8],
    width: usize,
}

impl IndexedDepNode<'_> {
    pub(crate) fn edges(&self) -> impl Iterator<Item = SerializedDepNodeIndex> + '_ {
        self.edges.chunks_exact(self.width).map(|chunk| {
            let mut bytes = [0; 4];
            bytes[..chunk.len()].copy_from_slice(chunk);
            SerializedDepNodeIndex::from_u32(u32::from_le_bytes(bytes))
        })
    }
}

impl<'a> IndexedDepGraph<'a> {
    pub(crate) fn new(data: &'a [u8], graph_start: usize) -> Option<Self> {
        let mut decoder = MemDecoder::new(data, graph_start).ok()?;
        let fixed = if SerializedDepGraph::has_fixed_check_marker(data, graph_start) {
            Some(fixed::Directory::new(data.get(..decoder.len())?, graph_start)?)
        } else {
            None
        };
        let index = if let Some(d) = fixed {
            SeekIndexMetadata {
                start: d.start,
                roots_start: d.roots_start,
                roots_len: d.roots_len,
                node_max: d.node_max,
                node_count: d.node_count,
            }
        } else {
            SeekIndexMetadata::decode(&mut decoder)?
        };
        if index.start < graph_start {
            return None;
        }
        Some(Self { data, graph_start, index, fixed, node_reads: Cell::new(0) })
    }

    pub(crate) fn node_count(&self) -> usize {
        self.index.node_count
    }

    pub(crate) fn node_reads(&self) -> usize {
        self.node_reads.get()
    }

    fn header(&self, index: SerializedDepNodeIndex) -> Option<(SerializedNodeHeader, usize)> {
        let raw_index = index.as_u32() as usize;
        if raw_index >= self.index.node_max {
            return None;
        }
        let slot = self.index.start.checked_add(raw_index.checked_mul(8)?)?;
        let offset = u64::from_le_bytes(self.data.get(slot..slot + 8)?.try_into().ok()?);
        let offset = usize::try_from(offset).ok()?;
        let end = offset.checked_add(size_of::<SerializedNodeHeader>())?;
        if offset < self.graph_start || end > self.index.start {
            return None;
        }
        let header = SerializedNodeHeader { bytes: self.data.get(offset..end)?.try_into().ok()? };
        if header.index() != index || header.node().kind == DepKind::Null {
            return None;
        }
        self.node_reads.set(self.node_reads.get() + 1);
        Some((header, end))
    }

    /// Only cacheable frontend roots have a key index; dependency nodes use their offsets.
    pub(crate) fn node_to_index_opt(&self, node: &DepNode) -> Option<SerializedDepNodeIndex> {
        let wanted = key(node);
        let mut left = 0;
        let mut right = self.index.roots_len;
        while left < right {
            let middle = left + (right - left) / 2;
            let offset = self.index.roots_start + middle * 4;
            let raw = u32::from_le_bytes(self.data.get(offset..offset + 4)?.try_into().ok()?);
            if raw > SerializedDepNodeIndex::MAX_AS_U32 || raw as usize >= self.index.node_max {
                return None;
            }
            let index = SerializedDepNodeIndex::from_u32(raw);
            let node = if let Some(d) = self.fixed {
                let (block, row) = d.indexed(index)?;
                let (node, _) = block.node(row)?;
                self.node_reads.set(self.node_reads.get() + 1);
                node
            } else {
                self.header(index)?.0.node()
            };
            match key(&node).cmp(&wanted) {
                std::cmp::Ordering::Less => left = middle + 1,
                std::cmp::Ordering::Greater => right = middle,
                std::cmp::Ordering::Equal => return Some(index),
            }
        }
        None
    }

    pub(crate) fn node(&self, index: SerializedDepNodeIndex) -> Option<IndexedDepNode<'a>> {
        if let Some(d) = self.fixed {
            let (block, row) = d.indexed(index)?;
            let (node, fingerprint) = block.node(row)?;
            let edges = block.row(row)?;
            if edges
                .as_chunks::<4>()
                .0
                .iter()
                .any(|&raw| u32::from_le_bytes(raw) as usize >= d.node_max)
            {
                return None;
            }
            self.node_reads.set(self.node_reads.get() + 1);
            return Some(IndexedDepNode { node, fingerprint, edges, width: 4 });
        }
        let (header, position) = self.header(index)?;
        let mut decoder = MemDecoder::new(self.data, position).ok()?;
        let len = header.len().unwrap_or_else(|| decoder.read_u32()) as usize;
        let width = header.bytes_per_index();
        let end = decoder.position().checked_add(len.checked_mul(width)?)?;
        if end > self.index.start {
            return None;
        }
        Some(IndexedDepNode {
            node: header.node(),
            fingerprint: header.value_fingerprint(),
            edges: self.data.get(decoder.position()..end)?,
            width,
        })
    }
}
