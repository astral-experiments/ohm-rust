//! Random access to a check's serialized graph, without materializing its nodes or edges.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

use rustc_data_structures::fingerprint::Fingerprint;
use rustc_index::IndexSlice;
use rustc_serialize::opaque::MemDecoder;
use rustc_serialize::{Decoder, Encoder};

use super::{
    DepKind, DepNode, DepNodeIndex, SerializedDepGraph, SerializedDepNodeIndex,
    SerializedNodeHeader,
};

const FOOTER_BYTES: usize = 24;
const INDEX_TRAILER_BYTES: usize = 24;
const INDEX_MAGIC: u64 = u64::from_le_bytes(*b"OHMIDX01");

fn key(node: &DepNode) -> (u16, [u8; 16]) {
    (node.kind.as_u16(), Fingerprint::from(node.key_fingerprint).to_le_bytes())
}

#[derive(Default)]
pub(super) struct SeekIndexBuilder {
    /// Little-endian file offsets, ready to write as one contiguous slice.
    offsets: Vec<[u8; 8]>,
    roots: Vec<(DepNode, DepNodeIndex)>,
}

impl SeekIndexBuilder {
    pub(super) fn reserve_slots(&mut self, len: usize) {
        if self.offsets.len() < len {
            self.offsets.resize(len, [0; 8]);
        }
    }

    /// Rows arrive in slot order, including holes and slots written by other workers.
    #[inline]
    pub(super) fn record_retained(&mut self, rows: impl ExactSizeIterator<Item = Option<usize>>) {
        self.reserve_slots(rows.len());
        for (slot, row) in self.offsets.iter_mut().zip(rows) {
            if let Some(offset) = row {
                debug_assert_eq!(*slot, [0; 8]);
                *slot = (offset as u64).to_le_bytes();
            }
        }
    }

    pub(super) fn record_chunk(
        &mut self,
        start: usize,
        slots: usize,
        offsets: &mut Vec<(DepNodeIndex, usize)>,
        roots: &mut Vec<(DepNode, DepNodeIndex)>,
    ) {
        self.reserve_slots(slots);
        for (index, offset) in offsets.drain(..) {
            debug_assert_eq!(self.offsets[index.as_usize()], [0; 8]);
            self.offsets[index.as_usize()] = u64::try_from(start + offset).unwrap().to_le_bytes();
        }
        self.roots.append(roots);
    }

    /// Merge newly written roots with retained entries from the old directory.
    /// A strictly increasing sequence of retained root keys with the expected
    /// count must contain every retained root exactly once.
    fn merge_roots(
        &self,
        previous: &SerializedDepGraph,
        retained: &IndexSlice<SerializedDepNodeIndex, AtomicBool>,
        count: usize,
    ) -> Option<Vec<[u8; 4]>> {
        let mut merged = Vec::with_capacity(count);
        let mut new = self.roots.iter().peekable();
        let mut last = None;
        for &slot in &previous.sorted_roots {
            let raw = u32::from_le_bytes(slot);
            if !retained.raw.get(raw as usize)?.load(Ordering::Relaxed) {
                continue;
            }
            let node = previous.index_to_node(SerializedDepNodeIndex::from_u32(raw));
            if !matches!(node.kind, DepKind::typeck_root | DepKind::mir_borrowck) {
                return None;
            }
            let current = key(node);
            if last.is_some_and(|last| last >= current) {
                return None;
            }
            last = Some(current);
            while let Some((_, index)) = new.next_if(|(node, _)| key(node) < current) {
                merged.push(index.as_u32().to_le_bytes());
            }
            merged.push(slot);
        }
        merged.extend(new.map(|(_, index)| index.as_u32().to_le_bytes()));
        (merged.len() == count).then_some(merged)
    }

    /// Appends an optional seek directory before the ordinary graph footer.
    pub(super) fn encode(
        &mut self,
        encoder: &mut impl Encoder,
        start: usize,
        node_max: u32,
        previous: &SerializedDepGraph,
        retained: &IndexSlice<SerializedDepNodeIndex, AtomicBool>,
        count: usize,
    ) {
        assert!(self.offsets.len() <= node_max as usize);
        self.reserve_slots(node_max as usize);
        encoder.emit_raw_bytes(self.offsets.as_flattened());
        self.roots.sort_unstable_by_key(|(node, _)| key(node));
        if self.roots.len() != count
            && let Some(merged) = self.merge_roots(previous, retained, count)
        {
            encoder.emit_raw_bytes(merged.as_flattened());
        } else {
            if self.roots.len() != count {
                // The previous graph may have no seek directory, or a
                // malformed one. Reconstruct it from the decoded rows.
                for (index, flag) in retained.iter_enumerated() {
                    if flag.load(Ordering::Relaxed) {
                        let node = previous.index_to_node(index);
                        if matches!(node.kind, DepKind::typeck_root | DepKind::mir_borrowck) {
                            self.roots.push((*node, DepNodeIndex::from_u32(index.as_u32())));
                        }
                    }
                }
                self.roots.sort_unstable_by_key(|(node, _)| key(node));
            }
            assert_eq!(self.roots.len(), count);
            for (_, index) in &self.roots {
                encoder.emit_raw_bytes(&index.as_u32().to_le_bytes());
            }
        }
        for value in [start as u64, count as u64, INDEX_MAGIC] {
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
    pub(super) fn roots(self, data: &[u8]) -> &[[u8; 4]] {
        // decode checked this span against the start of the trailer.
        data[self.roots_start..self.roots_start + self.roots_len * 4].as_chunks().0
    }

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
        let index = SeekIndexMetadata::decode(&mut decoder)?;
        if index.start < graph_start {
            return None;
        }
        Some(Self { data, graph_start, index, node_reads: Cell::new(0) })
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
            let index = SerializedDepNodeIndex::from_u32(u32::from_le_bytes(
                self.data.get(offset..offset + 4)?.try_into().ok()?,
            ));
            let (header, _) = self.header(index)?;
            match key(&header.node()).cmp(&wanted) {
                std::cmp::Ordering::Less => left = middle + 1,
                std::cmp::Ordering::Greater => right = middle,
                std::cmp::Ordering::Equal => return Some(index),
            }
        }
        None
    }

    pub(crate) fn node(&self, index: SerializedDepNodeIndex) -> Option<IndexedDepNode<'a>> {
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
