//! Check graphs have a seek index for reusing frontend roots during a later build.
//! Each worker owns a contiguous batch of node indices. Flush one batch at a time,
//! with its records, row offsets, and fixed-width edges in separate contiguous arrays.

use rustc_data_structures::fingerprint::Fingerprint;
use rustc_serialize::Encoder;

use super::index::key;
use super::{DepKind, DepNode, DepNodeIndex, EdgeHeader, NodeInfo, SerializedDepNodeIndex};

pub(super) const BLOCK_ROWS: u32 = 256;
// No valid packed graph begins with a serialized Null node. An empty packed graph has only
// zero-valued kind counts before the session number, so it cannot begin with this marker either.
const NULL: [u8; 2] = (DepKind::Null as u16).to_le_bytes();
pub(super) const MAGIC: &[u8; 8] = &[NULL[0], NULL[1], b'O', b'H', b'M', b'F', b'1', b'1'];
const RECORD_BYTES: usize = 34;
const BLOCK_HEADER_BYTES: usize = 16;
const TRAILER_BYTES: usize = 32;
const COUNTS_BYTES: usize = 24;

#[derive(Default)]
pub(super) struct BlockWriter {
    first: u32,
    records: Vec<[u8; RECORD_BYTES]>,
    offsets: Vec<usize>,
    edges: Vec<[u8; 4]>,
    roots: Vec<(DepNode, DepNodeIndex)>,
}

impl BlockWriter {
    /// Each index actually consumed from the worker's batch is consecutive. An unused
    /// tail stays implicit when the last block is flushed at the end of compilation.
    pub(super) fn push(&mut self, index: DepNodeIndex, info: &NodeInfo<'_>) -> bool {
        if self.records.is_empty() {
            self.first = index.as_u32();
            debug_assert_eq!(self.first % BLOCK_ROWS, 0);
        }
        debug_assert_eq!(index.as_u32() - self.first, self.records.len() as u32);
        let mut record = [0; RECORD_BYTES];
        record[..2].copy_from_slice(&info.node.kind.as_u16().to_le_bytes());
        record[2..18].copy_from_slice(&Fingerprint::from(info.node.key_fingerprint).to_le_bytes());
        record[18..].copy_from_slice(&info.value_fingerprint.to_le_bytes());
        self.records.push(record);
        self.offsets.push(self.edges.len());
        self.edges.extend(info.edges.iter().map(|e| e.as_u32().to_le_bytes()));
        if matches!(info.node.kind, DepKind::typeck_root | DepKind::mir_borrowck) {
            self.roots.push((info.node, index));
        }
        self.records.len() == BLOCK_ROWS as usize
    }

    pub(super) fn flush(
        &mut self,
        e: &mut impl Encoder,
        position: usize,
        index: &mut DirectoryWriter,
    ) {
        if self.records.is_empty() {
            return;
        }
        let block = (self.first / BLOCK_ROWS) as usize;
        if block >= index.blocks.len() {
            index.blocks.resize(block + 1, 0);
        }
        debug_assert_eq!(index.blocks[block], 0);
        index.blocks[block] = position as u64;
        index.roots.append(&mut self.roots);
        e.emit_raw_bytes(&self.first.to_le_bytes());
        e.emit_raw_bytes(&(self.records.len() as u32).to_le_bytes());
        e.emit_raw_bytes(&(self.edges.len() as u64).to_le_bytes());
        e.emit_raw_bytes(self.records.as_flattened());
        self.offsets.push(self.edges.len());
        if self.edges.len() > u32::MAX as usize {
            for offset in &self.offsets {
                e.emit_raw_bytes(&(*offset as u64).to_le_bytes());
            }
        } else {
            for offset in &self.offsets {
                e.emit_raw_bytes(&(*offset as u32).to_le_bytes());
            }
        }
        e.emit_raw_bytes(self.edges.as_flattened());
        self.records.clear();
        self.offsets.clear();
        self.edges.clear();
        // A single high-fanout block must not keep its scratch allocation for the rest of the compile.
        if self.edges.capacity() > 64 * 1024 {
            self.edges = Vec::new();
        }
    }
}

#[derive(Default)]
pub(super) struct DirectoryWriter {
    blocks: Vec<u64>,
    roots: Vec<(DepNode, DepNodeIndex)>,
}

impl DirectoryWriter {
    pub(super) fn encode(
        &mut self,
        e: &mut impl Encoder,
        stats: usize,
        start: usize,
        node_max: u32,
    ) {
        self.blocks.resize(node_max.div_ceil(BLOCK_ROWS) as usize, 0);
        for position in &self.blocks {
            e.emit_raw_bytes(&position.to_le_bytes());
        }
        self.roots.sort_unstable_by_key(|(node, _)| key(node));
        for (_, index) in &self.roots {
            e.emit_raw_bytes(&index.as_u32().to_le_bytes());
        }
        for value in [stats as u64, start as u64, self.roots.len() as u64] {
            e.emit_raw_bytes(&value.to_le_bytes());
        }
        e.emit_raw_bytes(MAGIC);
    }
}

fn word<const N: usize>(data: &[u8], at: usize) -> Option<[u8; N]> {
    data.get(at..)?.first_chunk().copied()
}

fn size(data: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u64::from_le_bytes(word(data, at)?)).ok()
}

/// A directory borrows the file. Random lookups access a block header and the requested
/// record or row without owning any part of the graph. The eager decoder copies the arrays
/// it needs before releasing the original mapping.
#[derive(Clone, Copy)]
pub(super) struct Directory<'a> {
    data: &'a [u8],
    pub(super) graph_start: usize,
    pub(super) stats: usize,
    pub(super) start: usize,
    pub(super) roots_start: usize,
    pub(super) roots_len: usize,
    pub(super) node_max: usize,
    pub(super) node_count: usize,
    pub(super) edge_count: usize,
}

impl<'a> Directory<'a> {
    /// The input ends before the ordinary encoder's end-of-file marker.
    pub(super) fn new(data: &'a [u8], graph_start: usize) -> Option<Self> {
        if word::<8>(data, graph_start)? != *MAGIC {
            return None;
        }
        let footer = data.len().checked_sub(COUNTS_BYTES)?;
        let trailer = footer.checked_sub(TRAILER_BYTES)?;
        if word::<8>(data, trailer + 24)? != *MAGIC {
            return None;
        }
        let stats = size(data, trailer)?;
        let start = size(data, trailer + 8)?;
        let roots_len = size(data, trailer + 16)?;
        let node_max = size(data, footer)?;
        let node_count = size(data, footer + 8)?;
        let edge_count = size(data, footer + 16)?;
        let graph_start = graph_start.checked_add(MAGIC.len())?;
        let blocks = node_max.div_ceil(BLOCK_ROWS as usize);
        let roots_start = start.checked_add(blocks.checked_mul(8)?)?;
        if roots_start.checked_add(roots_len.checked_mul(4)?)? != trailer
            || stats < graph_start
            || stats > start
            || node_max > SerializedDepNodeIndex::MAX_AS_U32 as usize + 1
            || node_count > node_max
            || roots_len > node_count
            || node_count.checked_mul(RECORD_BYTES)?.checked_add(edge_count.checked_mul(4)?)?
                > stats - graph_start
        {
            return None;
        }
        Some(Self {
            data,
            graph_start,
            stats,
            start,
            roots_start,
            roots_len,
            node_max,
            node_count,
            edge_count,
        })
    }

    pub(super) fn block_offset(&self, block: usize) -> Option<usize> {
        if block >= self.node_max.div_ceil(BLOCK_ROWS as usize) {
            return None;
        }
        size(self.data, self.start + block * 8)
    }

    pub(super) fn block(&self, block: usize) -> Option<Block<'a>> {
        let position = self.block_offset(block)?;
        if !(self.graph_start..self.stats).contains(&position) {
            return None;
        }
        let data = self.data.get(position..self.stats)?;
        let first = u32::from_le_bytes(word(data, 0)?) as usize;
        let rows = u32::from_le_bytes(word(data, 4)?) as usize;
        let edges = size(data, 8)?;
        if first != block * BLOCK_ROWS as usize
            || rows == 0
            || rows > (self.node_max - first).min(BLOCK_ROWS as usize)
        {
            return None;
        }
        let width = if edges > u32::MAX as usize { 8 } else { 4 };
        let records_end = BLOCK_HEADER_BYTES + rows * RECORD_BYTES;
        let offsets_end = records_end + (rows + 1) * width;
        let end = offsets_end.checked_add(edges.checked_mul(4)?)?;
        let block = Block {
            records: data.get(BLOCK_HEADER_BYTES..records_end)?,
            offsets: data.get(records_end..offsets_end)?,
            edges: data.get(offsets_end..end)?,
            width,
            rows,
            end: position + end,
        };
        if block.offset(0)? != 0 || block.offset(rows)? != edges {
            return None;
        }
        Some(block)
    }

    pub(super) fn indexed(&self, index: SerializedDepNodeIndex) -> Option<(Block<'a>, usize)> {
        let n = index.as_u32() as usize;
        let block = self.block(n / BLOCK_ROWS as usize)?;
        let row = n % BLOCK_ROWS as usize;
        (row < block.rows).then_some((block, row))
    }
}

pub(super) struct Block<'a> {
    records: &'a [u8],
    offsets: &'a [u8],
    pub(super) edges: &'a [u8],
    width: usize,
    pub(super) rows: usize,
    pub(super) end: usize,
}

impl<'a> Block<'a> {
    pub(super) fn node(&self, row: usize) -> Option<(DepNode, Fingerprint)> {
        let record =
            self.records.get(row.checked_mul(RECORD_BYTES)?..)?.first_chunk::<RECORD_BYTES>()?;
        let kind = u16::from_le_bytes(word(record, 0)?);
        if kind > DepKind::MAX || kind == DepKind::Null.as_u16() {
            return None;
        }
        Some((
            DepNode {
                kind: DepKind::from_u16(kind),
                key_fingerprint: Fingerprint::from_le_bytes(word(record, 2)?).into(),
            },
            Fingerprint::from_le_bytes(word(record, 18)?),
        ))
    }

    pub(super) fn offset(&self, row: usize) -> Option<usize> {
        let at = row.checked_mul(self.width)?;
        if self.width == 8 {
            size(self.offsets, at)
        } else {
            Some(u32::from_le_bytes(word(self.offsets, at)?) as usize)
        }
    }

    pub(super) fn row(&self, row: usize) -> Option<&'a [u8]> {
        self.edges.get(self.offset(row)?.checked_mul(4)?..self.offset(row + 1)?.checked_mul(4)?)
    }
}

#[derive(Debug)]
pub(super) struct OwnedEdges {
    pub(super) headers: Vec<EdgeHeader>,
    pub(super) targets: Vec<u8>,
}

impl Directory<'_> {
    /// Validate the physical block boundaries before allocating arrays indexed by the file's
    /// counts. The per-block row counts delimit every contiguous run of live nodes.
    pub(super) fn live_rows(&self) -> Option<Vec<u16>> {
        let mut rows = vec![0; self.node_max.div_ceil(BLOCK_ROWS as usize)];
        let mut position = self.graph_start;
        let mut node_count = 0usize;
        let mut edge_count = 0usize;
        while position < self.stats {
            let first = u32::from_le_bytes(word(self.data, position)?);
            let index = (first / BLOCK_ROWS) as usize;
            if self.block_offset(index)? != position {
                return None;
            }
            let block = self.block(index)?;
            rows[index] = block.rows as u16;
            node_count = node_count.checked_add(block.rows)?;
            edge_count = edge_count.checked_add(block.edges.len() / 4)?;
            position = block.end;
        }
        if node_count != self.node_count || edge_count != self.edge_count {
            return None;
        }
        for (index, &live) in rows.iter().enumerate() {
            if (live == 0) != (self.block_offset(index)? == 0) {
                return None;
            }
        }
        let mut stats = self.data.get(self.stats..self.start)?;
        for _ in 0..DepKind::NUM_VARIANTS {
            leb(&mut stats, u64::from(u32::MAX))?;
        }
        leb(&mut stats, u64::MAX)?;
        stats.is_empty().then_some(rows)
    }

    pub(super) fn read(
        &self,
        nodes: &mut [DepNode],
        values: &mut [Fingerprint],
        live: &[u16],
    ) -> Option<OwnedEdges> {
        let edge_bytes = self.edge_count.checked_mul(super::DEP_NODE_SIZE)?;
        if edge_bytes > usize::MAX >> super::DEP_NODE_WIDTH_BITS {
            return None;
        }
        let mut headers = vec![EdgeHeader { repr: 0, num_edges: 0 }; self.node_max];
        let mut targets = Vec::with_capacity(edge_bytes.checked_add(super::DEP_NODE_PAD)?);
        for (index, &count) in live.iter().enumerate() {
            let first = index * BLOCK_ROWS as usize;
            let row_end = first + usize::from(count);
            if count != 0 {
                let block = self.block(index)?;
                let nodes = &mut nodes[first..row_end];
                let values = &mut values[first..row_end];
                for ((node, value), record) in
                    nodes.iter_mut().zip(values).zip(block.records.as_chunks::<RECORD_BYTES>().0)
                {
                    let kind = u16::from_le_bytes(record[..2].try_into().unwrap());
                    if kind == DepKind::Null.as_u16() || kind > DepKind::MAX {
                        return None;
                    }
                    *node = DepNode {
                        kind: DepKind::from_u16(kind),
                        key_fingerprint: Fingerprint::from_le_bytes(
                            record[2..18].try_into().unwrap(),
                        )
                        .into(),
                    };
                    *value = Fingerprint::from_le_bytes(record[18..].try_into().unwrap());
                }
                let base = targets.len();
                let headers = &mut headers[first..row_end];
                let edge_count = block.edges.len() / 4;
                if block.width == 4 {
                    fill_headers(
                        headers,
                        base,
                        edge_count,
                        block.offsets[4..]
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .map(|&raw| u32::from_le_bytes(raw) as usize),
                    )?;
                } else {
                    fill_headers(
                        headers,
                        base,
                        edge_count,
                        block.offsets[8..]
                            .as_chunks::<8>()
                            .0
                            .iter()
                            .map(|&raw| u64::from_le_bytes(raw) as usize),
                    )?;
                }
                targets.extend_from_slice(block.edges);
                if block
                    .edges
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|&raw| u32::from_le_bytes(raw))
                    .max()
                    .is_some_and(|id| id as usize >= self.node_max)
                {
                    return None;
                }
            }
        }
        targets.extend_from_slice(&[0; super::DEP_NODE_PAD]);
        Some(OwnedEdges { headers, targets })
    }
}

fn fill_headers(
    dest: &mut [EdgeHeader],
    base: usize,
    count: usize,
    ends: impl Iterator<Item = usize>,
) -> Option<()> {
    let mut start = 0;
    for (dest, end) in dest.iter_mut().zip(ends) {
        if end < start || end > count {
            return None;
        }
        // Reuse the packed graph's cursor so serialization never adds a branch per edge.
        *dest = EdgeHeader {
            repr: ((base + super::DEP_NODE_SIZE * start) << super::DEP_NODE_WIDTH_BITS)
                | (super::DEP_NODE_SIZE - 1),
            num_edges: u32::try_from(end - start).ok()?,
        };
        start = end;
    }
    Some(())
}

/// Bound the variable-length kind counts before handing the validated section to MemDecoder.
fn leb(input: &mut &[u8], limit: u64) -> Option<u64> {
    let mut value = 0;
    for shift in (0..(u64::BITS - limit.leading_zeros())).step_by(7) {
        let (&byte, rest) = input.split_first()?;
        *input = rest;
        let digit = u64::from(byte & 0x7f);
        if digit > limit >> shift {
            return None;
        }
        value |= digit << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}
