//! The data that we will serialize and deserialize.
//!
//! Each row of the graph contains a node header and its outgoing edges. The
//! footer records the size of the index space and the number of nodes and
//! edges, so we can allocate the lookup tables before reading the rows.
//!
//! New and re-executed nodes are serialized on demand. When we keep the
//! previous indices, validation marks rows to copy when the session ends.
//!
//! The previous graph holds the packed rows and two arrays indexed by node
//! number. One stores node kinds and keys for validation and reverse lookup.
//! The other locates each node's value fingerprint and edges in its row, and
//! links nodes that share a bucket in the reverse index.
//!
//! The encoding of the dep-graph is generally designed around the fact that fixed-size
//! reads of encoded data are generally faster than variable-sized reads. Ergo we adopt
//! essentially the same varint encoding scheme used in the rmeta format; the edge lists
//! for each node on the graph store a 2-bit integer which is the number of bytes per edge
//! index in that node's edge list. We effectively ignore that an edge index of 0 could be
//! encoded with 0 bytes in order to not require 3 bits to store the byte width of the edges.
//! We determine the common byte width once per node by taking the bitwise OR of the
//! edge indices and counting its leading zero bytes.
//!
//! When we decode this data, we do not immediately create [`SerializedDepNodeIndex`] and
//! instead keep the data in its denser serialized form which lets us turn our on-disk size
//! efficiency directly into a peak memory reduction. When we convert these encoded-in-memory
//! values into their fully-deserialized type, we use a fixed-size read of the encoded array
//! then mask off any errant bytes we read. The array of edge index bytes is padded to permit this.
//!
//! We also encode and decode the entire rest of each node using [`SerializedNodeHeader`]
//! to let this encoding and decoding be done in one fixed-size operation. These headers contain
//! two [`Fingerprint`]s along with the serialized [`DepKind`], and the number of edge indices
//! in the node and the number of bytes used to encode the edge indices for this node. The
//! [`DepKind`], number of edges, and bytes per edge are all bit-packed together, if they fit.
//! If the number of edges in this node does not fit in the bits available in the header, we
//! store it directly after the header with leb128.
//!
//! Threads bulk allocate fresh indices in `LocalEncoderState`. Reusing a row
//! requires preserving every index in its edge list. When the previous index
//! space gets too sparse, we assign fresh indices to compact the graph.

use std::cell::RefCell;
use std::cmp::max;
use std::hash::{BuildHasher, BuildHasherDefault};
use std::iter;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

mod index;
#[cfg(test)]
mod tests;
pub(super) use index::IndexedDepGraph;
use index::SeekIndexBuilder;
use rustc_data_structures::fingerprint::{Fingerprint, PackedFingerprint};
use rustc_data_structures::fx::FxHashMap;
use rustc_data_structures::memmap::Mmap;
use rustc_data_structures::outline;
use rustc_data_structures::owned_slice::{OwnedSlice, slice_owned};
use rustc_data_structures::profiling::SelfProfilerRef;
use rustc_data_structures::sync::{AtomicU64, Lock, WorkerLocal, broadcast};
use rustc_data_structures::unhash::Unhasher;
use rustc_index::{IndexSlice, IndexVec};
use rustc_serialize::opaque::mem_encoder::MemEncoder;
use rustc_serialize::opaque::{FileEncodeResult, FileEncoder, IntEncodedWithFixedSize, MemDecoder};
use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};
use rustc_session::Session;
use tracing::{debug, instrument};

use super::graph::{CurrentDepGraph, DepNodeColorMap, DesiredColor, TrySetColorResult};
use super::retained::RetainedDepGraph;
use super::{DepKind, DepNode, DepNodeIndex};

// The maximum value of `SerializedDepNodeIndex` leaves the upper two bits
// unused so that we can store multiple index types in `CompressedHybridIndex`,
// and use those bits to encode which index type it contains.
rustc_index::newtype_index! {
    #[encodable]
    #[max = 0x7FFF_FFFF]
    pub struct SerializedDepNodeIndex {}
}

impl SerializedDepNodeIndex {
    /// Converts a current-session dep node index to a "serialized" index,
    /// for the purpose of serializing data to be loaded by future sessions.
    #[inline(always)]
    pub fn from_curr_for_serialization(index: DepNodeIndex) -> Self {
        SerializedDepNodeIndex::from_u32(index.as_u32())
    }
}

const DEP_NODE_SIZE: usize = size_of::<SerializedDepNodeIndex>();
/// Amount of padding we need to add to the edge list data so that we can retrieve every
/// SerializedDepNodeIndex with a fixed-size read then mask.
const DEP_NODE_PAD: usize = DEP_NODE_SIZE - 1;
/// Number of bits we need to store the number of used bytes in a SerializedDepNodeIndex.
/// Note that wherever we encode byte widths like this we actually store the number of bytes used
/// minus 1; for a 4-byte value we technically would have 5 widths to store, but using one byte to
/// store zeroes (which are relatively rare) is a decent tradeoff to save a bit in our bitfields.
const DEP_NODE_WIDTH_BITS: usize = DEP_NODE_SIZE / 2;

/// Data for use when recompiling the **current crate**.
///
/// Removed nodes and unused entries in per-thread index batches leave holes
/// marked by `DepKind::Null`.
#[derive(Default)]
pub struct SerializedDepGraph {
    /// The set of all DepNodes in the graph
    nodes: IndexVec<SerializedDepNodeIndex, DepNode>,
    /// Locations of headers and edges inside the serialized rows. A missing row
    /// corresponds to a null slot in `nodes`.
    row_index: IndexVec<SerializedDepNodeIndex, RowIndex>,
    /// The contiguous rows, followed by padding for edge reads. The ordinary
    /// graph footer supplies this padding. Empty graphs have no backing file.
    rows: Option<OwnedSlice>,
    /// The head of each reverse lookup chain. Its links live in `row_index`.
    buckets: Vec<Option<SerializedDepNodeIndex>>,
    /// The number of nodes actually encoded. [`Self::index_space_len`] also
    /// counts the holes.
    live_node_count: usize,
    /// The number of previous compilation sessions. This is used to generate
    /// unique anon dep nodes per session.
    session_count: u64,
}

// Omit the packed bytes in `OwnedSlice` from the debug output.
impl std::fmt::Debug for SerializedDepGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SerializedDepGraph")
            .field("nodes", &self.nodes)
            .field("row_index", &self.row_index)
            .field("buckets", &self.buckets)
            .field("live_node_count", &self.live_node_count)
            .field("session_count", &self.session_count)
            .finish_non_exhaustive()
    }
}

#[inline]
fn fingerprint_hash(fingerprint: PackedFingerprint) -> u64 {
    BuildHasherDefault::<Unhasher>::default().hash_one(fingerprint)
}

fn reverse_bucket(node: &DepNode, size: usize) -> usize {
    // Mix in the kind because many queries have the same definition as their key.
    let hash = fingerprint_hash(node.key_fingerprint) as usize;
    (hash ^ node.kind.as_usize().wrapping_mul(0x9e3779b9)) & (size - 1)
}

/// Index the rows by kind and key in one pass. Looking up a node compares the
/// full key, and inserting indices in ascending order puts the last side effect
/// with a given key first in its chain.
fn build_reverse_index(
    nodes: &IndexSlice<SerializedDepNodeIndex, DepNode>,
    rows: &mut IndexSlice<SerializedDepNodeIndex, RowIndex>,
    count: usize,
) -> Vec<Option<SerializedDepNodeIndex>> {
    let mut heads: Vec<Option<SerializedDepNodeIndex>> = vec![None; count.next_power_of_two()];
    for (index, node) in nodes.iter_enumerated() {
        if node.kind == DepKind::Null {
            continue;
        }
        let bucket = reverse_bucket(node, heads.len());
        let mut next = heads[bucket];
        // Side effects deliberately share their key. Keep only the latest
        // duplicate at the head of a chain.
        if node.kind == DepKind::SideEffect
            && let Some(last) = next
            && nodes[last] == *node
        {
            next = rows[last].next;
        }
        rows[index].next = next;
        heads[bucket] = Some(index);
    }
    heads
}

impl SerializedDepGraph {
    #[inline]
    fn row_bytes(&self) -> &[u8] {
        self.rows.as_deref().unwrap()
    }

    #[inline]
    fn header_at(&self, start: usize) -> SerializedNodeHeader {
        SerializedNodeHeader { bytes: *self.row_bytes()[start..].first_chunk().unwrap() }
    }

    #[inline]
    pub fn edge_targets_from(
        &self,
        source: SerializedDepNodeIndex,
    ) -> impl Iterator<Item = SerializedDepNodeIndex> + Clone {
        let header = self.row_index[source];
        let bytes_per_index = header.width as usize;
        let edge_bytes = header.num_edges as usize * bytes_per_index;
        // The padding after the last edge is too short to read another index.
        let mut raw = &self.row_bytes()[header.edges_start..][..edge_bytes + DEP_NODE_PAD];

        let mask = mask(bytes_per_index * 8) as u32;
        iter::from_fn(move || {
            let index = u32::from_le_bytes(*raw.first_chunk::<DEP_NODE_SIZE>()?) & mask;
            raw = &raw[bytes_per_index..];
            Some(SerializedDepNodeIndex::from_u32(index))
        })
    }

    /// Visit a packed row in order, stopping at the first dependency that cannot
    /// be marked green. Dispatch once so the hot loop uses fixed-size loads.
    #[inline(always)]
    pub(super) fn try_for_each_edge(
        &self,
        source: SerializedDepNodeIndex,
        visit: impl FnMut(SerializedDepNodeIndex) -> Option<()>,
    ) -> Option<()> {
        #[inline(always)]
        fn walk<const WIDTH: usize>(
            bytes: &[u8],
            mut visit: impl FnMut(SerializedDepNodeIndex) -> Option<()>,
        ) -> Option<()> {
            let (edges, tail) = bytes.as_chunks::<WIDTH>();
            debug_assert!(tail.is_empty());
            for edge in edges {
                let mut index = [0; DEP_NODE_SIZE];
                index[..WIDTH].copy_from_slice(edge);
                visit(SerializedDepNodeIndex::from_u32(u32::from_le_bytes(index)))?;
            }
            Some(())
        }

        let row = self.row_index[source];
        let bytes = &self.row_bytes()[row.edges_start..row.end()];
        match row.width {
            1 => walk::<1>(bytes, visit),
            2 => walk::<2>(bytes, visit),
            3 => walk::<3>(bytes, visit),
            4 => walk::<4>(bytes, visit),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub(super) fn has_same_short_edges(
        &self,
        index: SerializedDepNodeIndex,
        edges: &[DepNodeIndex],
    ) -> bool {
        match (self.row_index[index].num_edges, edges) {
            (0, []) => true,
            (1, [edge]) => self.edge_targets_from(index).next().unwrap().as_u32() == edge.as_u32(),
            _ => false,
        }
    }

    #[inline]
    pub fn index_to_node(&self, dep_node_index: SerializedDepNodeIndex) -> &DepNode {
        &self.nodes[dep_node_index]
    }

    #[inline]
    pub fn node_to_index_opt(&self, dep_node: &DepNode) -> Option<SerializedDepNodeIndex> {
        if self.buckets.is_empty() {
            return None;
        }
        let mut next = self.buckets[reverse_bucket(dep_node, self.buckets.len())];
        let mut found = None;
        while let Some(i) = next {
            if self.nodes[i] == *dep_node {
                if dep_node.kind == DepKind::SideEffect {
                    return Some(i);
                }
                assert!(
                    found.is_none(),
                    "Error: A dep graph node ({:?}) does not have an unique index. \
                     Running a clean build on a nightly compiler with \
                     `-Z incremental-verify-ich` can help narrow down the issue for reporting. \
                     A clean build may also work around the issue.\n
                     DepNode: {dep_node:?}",
                    dep_node.kind
                );
                found = Some(i);
            }
            next = self.row_index[i].next;
        }
        found
    }

    #[inline]
    pub fn value_fingerprint_for_index(
        &self,
        dep_node_index: SerializedDepNodeIndex,
    ) -> Fingerprint {
        let row = self.row_index[dep_node_index];
        if row.header_len == 0 {
            return Fingerprint::ZERO;
        }
        self.header_at(row.edges_start - usize::from(row.header_len)).value_fingerprint()
    }

    /// The number of dep-node indices, counting those that hold no node.
    #[inline]
    pub fn index_space_len(&self) -> usize {
        self.nodes.len()
    }

    #[inline]
    pub fn live_node_count(&self) -> usize {
        self.live_node_count
    }

    #[inline]
    pub fn session_count(&self) -> u64 {
        self.session_count
    }
}

/// Location of one serialized row. The length stored before the edges can
/// spill out of the fixed header into a varint.
#[derive(Debug, Clone, Copy, Default)]
struct RowIndex {
    edges_start: usize,
    num_edges: u32,
    header_len: u8,
    width: u8,
    next: Option<SerializedDepNodeIndex>,
}

impl RowIndex {
    fn end(self) -> usize {
        self.edges_start + self.num_edges as usize * usize::from(self.width)
    }
}

#[inline]
fn mask(bits: usize) -> usize {
    usize::MAX >> ((size_of::<usize>() * 8) - bits)
}

impl SerializedDepGraph {
    /// Returns `None` if an Ohm stage graph contains a null, duplicate or out-of-bounds node.
    #[instrument(level = "debug", skip(bytes, profiler))]
    pub fn try_decode(bytes: Mmap, start: usize, profiler: &SelfProfilerRef) -> Option<Arc<Self>> {
        let mut decoder = MemDecoder::new(&bytes, start).ok()?;
        let d = &mut decoder;
        // The last 16 bytes are the node count and edge count.
        debug!("position: {:?}", d.position());

        // `node_max` is the number of indices including empty nodes while `node_count`
        // is the number of actually encoded nodes.
        let (node_max, node_count, edge_count) =
            d.with_position(d.len() - 3 * IntEncodedWithFixedSize::ENCODED_SIZE, |d| {
                debug!("position: {:?}", d.position());
                let node_max = IntEncodedWithFixedSize::decode(d).0 as usize;
                let node_count = IntEncodedWithFixedSize::decode(d).0 as usize;
                let edge_count = IntEncodedWithFixedSize::decode(d).0 as usize;
                (node_max, node_count, edge_count)
            });
        debug!("position: {:?}", d.position());

        debug!(?node_count, ?edge_count);

        let mut nodes = IndexVec::from_elem_n(
            DepNode {
                kind: DepKind::Null,
                key_fingerprint: PackedFingerprint::from(Fingerprint::ZERO),
            },
            node_max,
        );
        let mut row_index = IndexVec::from_elem_n(RowIndex::default(), node_max);
        for _ in 0..node_count {
            let row_start = d.position();
            // Decode the header for this edge; the header packs together as many of the fixed-size
            // fields as possible to limit the number of times we update decoder state.
            let node_header = SerializedNodeHeader { bytes: d.read_array() };

            let index = node_header.index();

            let node = nodes.get_mut(index)?;
            let decoded = node_header.node();
            if decoded.kind == DepKind::Null || node.kind != DepKind::Null {
                return None;
            }
            *node = decoded;

            // If the length of this node's edge list is small, the length is stored in the header.
            // If it is not, we fall back to another decoder call.
            let num_edges = node_header.len().unwrap_or_else(|| d.read_u32());

            let width = node_header.bytes_per_index();
            row_index[index] = RowIndex {
                edges_start: d.position() - start,
                num_edges,
                header_len: (d.position() - row_start).try_into().unwrap(),
                width: width as u8,
                next: None,
            };
            let _ = d.read_raw_bytes(width * num_edges as usize);
        }
        let padded_end = d.position() + DEP_NODE_PAD;
        let mut total = 0;
        for _ in 0..DepKind::NUM_VARIANTS {
            total += d.read_u32() as usize;
        }
        debug_assert_eq!(total, node_count);
        let session_count = d.read_u64();
        let buckets = {
            let _timer = profiler.generic_activity("incr_comp_load_dep_graph_reverse_index");
            build_reverse_index(&nodes, &mut row_index, node_count)
        };
        // On Windows finish reading the old file before replacing it.
        #[cfg(windows)]
        let rows = slice_owned(bytes[start..padded_end].to_vec(), |data| &data[..]);
        #[cfg(not(windows))]
        let rows = slice_owned(bytes, |data| &data[start..padded_end]);
        Some(Arc::new(Self {
            nodes,
            row_index,
            rows: Some(rows),
            buckets,
            live_node_count: node_count,
            session_count,
        }))
    }
}

/// A packed representation of all the fixed-size fields in a `NodeInfo`.
///
/// This stores in one byte array:
/// * The value `Fingerprint` in the `NodeInfo`
/// * The key `Fingerprint` in `DepNode` that is in this `NodeInfo`
/// * The `DepKind`'s discriminant (a u16, but not all bits are used...)
/// * The byte width of the encoded edges for this node
/// * In whatever bits remain, the length of the edge list for this node, if it fits
struct SerializedNodeHeader {
    // 2 bytes for the DepNode
    // 4 bytes for the index
    // 16 for Fingerprint in DepNode
    // 16 for Fingerprint in NodeInfo
    bytes: [u8; 38],
}

// The fields of a `SerializedNodeHeader`, this struct is an implementation detail and exists only
// to make the implementation of `SerializedNodeHeader` simpler.
struct Unpacked {
    len: Option<u32>,
    bytes_per_index: usize,
    kind: DepKind,
    index: SerializedDepNodeIndex,
    key_fingerprint: PackedFingerprint,
    value_fingerprint: Fingerprint,
}

// Bit fields, where
// M: bits used to store the length of a node's edge list
// N: bits used to store the byte width of elements of the edge list
// are
// 0..M    length of the edge
// M..M+N  bytes per index
// M+N..16 kind
impl SerializedNodeHeader {
    const TOTAL_BITS: usize = size_of::<DepKind>() * 8;
    const LEN_BITS: usize = Self::TOTAL_BITS - Self::KIND_BITS - Self::WIDTH_BITS;
    const WIDTH_BITS: usize = DEP_NODE_WIDTH_BITS;
    const KIND_BITS: usize = Self::TOTAL_BITS - DepKind::MAX.leading_zeros() as usize;
    const MAX_INLINE_LEN: usize = (u16::MAX as usize >> (Self::TOTAL_BITS - Self::LEN_BITS)) - 1;

    #[inline]
    fn new(
        node: &DepNode,
        index: DepNodeIndex,
        value_fingerprint: Fingerprint,
        edge_bits: u32,
        edge_count: usize,
    ) -> Self {
        debug_assert_eq!(Self::TOTAL_BITS, Self::LEN_BITS + Self::WIDTH_BITS + Self::KIND_BITS);

        let mut head = node.kind.as_u16();

        let free_bytes = edge_bits.leading_zeros() as usize / 8;
        let bytes_per_index = (DEP_NODE_SIZE - free_bytes).saturating_sub(1);
        head |= (bytes_per_index as u16) << Self::KIND_BITS;

        // Encode number of edges + 1 so that we can reserve 0 to indicate that the len doesn't fit
        // in this bitfield.
        if edge_count <= Self::MAX_INLINE_LEN {
            head |= (edge_count as u16 + 1) << (Self::KIND_BITS + Self::WIDTH_BITS);
        }

        let hash: Fingerprint = node.key_fingerprint.into();

        // Using half-open ranges ensures an unconditional panic if we get the magic numbers wrong.
        let mut bytes = [0u8; 38];
        bytes[..2].copy_from_slice(&head.to_le_bytes());
        bytes[2..6].copy_from_slice(&index.as_u32().to_le_bytes());
        bytes[6..22].copy_from_slice(&hash.to_le_bytes());
        bytes[22..].copy_from_slice(&value_fingerprint.to_le_bytes());

        #[cfg(debug_assertions)]
        {
            let res = Self { bytes };
            assert_eq!(value_fingerprint, res.value_fingerprint());
            assert_eq!(*node, res.node());
            if let Some(len) = res.len() {
                assert_eq!(edge_count, len as usize);
            }
        }
        Self { bytes }
    }

    #[inline]
    fn unpack(&self) -> Unpacked {
        let head = u16::from_le_bytes(self.bytes[..2].try_into().unwrap());
        let index = u32::from_le_bytes(self.bytes[2..6].try_into().unwrap());
        let key_fingerprint = self.bytes[6..22].try_into().unwrap();
        let value_fingerprint = self.bytes[22..].try_into().unwrap();

        let kind = head & mask(Self::KIND_BITS) as u16;
        let bytes_per_index = (head >> Self::KIND_BITS) & mask(Self::WIDTH_BITS) as u16;
        let len = (head as u32) >> (Self::WIDTH_BITS + Self::KIND_BITS);

        Unpacked {
            len: len.checked_sub(1),
            bytes_per_index: bytes_per_index as usize + 1,
            kind: DepKind::from_u16(kind),
            index: SerializedDepNodeIndex::from_u32(index),
            key_fingerprint: Fingerprint::from_le_bytes(key_fingerprint).into(),
            value_fingerprint: Fingerprint::from_le_bytes(value_fingerprint),
        }
    }

    #[inline]
    fn len(&self) -> Option<u32> {
        self.unpack().len
    }

    #[inline]
    fn bytes_per_index(&self) -> usize {
        self.unpack().bytes_per_index
    }

    #[inline]
    fn index(&self) -> SerializedDepNodeIndex {
        self.unpack().index
    }

    #[inline]
    fn value_fingerprint(&self) -> Fingerprint {
        self.unpack().value_fingerprint
    }

    #[inline]
    fn node(&self) -> DepNode {
        let Unpacked { kind, key_fingerprint, .. } = self.unpack();
        DepNode { kind, key_fingerprint }
    }
}

#[derive(Debug)]
struct NodeInfo<'a> {
    node: DepNode,
    value_fingerprint: Fingerprint,
    edges: &'a [DepNodeIndex],
}

impl NodeInfo<'_> {
    fn encode(&self, e: &mut MemEncoder, index: DepNodeIndex) {
        let NodeInfo { ref node, value_fingerprint, edges } = *self;
        // The highest bit set in any index determines the byte width of the list.
        let edge_bits = edges.iter().fold(0, |bits, edge| bits | edge.as_u32());
        let header =
            SerializedNodeHeader::new(node, index, value_fingerprint, edge_bits, edges.len());
        e.write_array(header.bytes);

        if header.len().is_none() {
            // The edges are all unique and the number of unique indices is less than u32::MAX.
            e.emit_u32(edges.len().try_into().unwrap());
        }

        let bytes_per_index = header.bytes_per_index();
        for node_index in edges.iter() {
            e.write_with(|dest| {
                *dest = node_index.as_u32().to_le_bytes();
                bytes_per_index
            });
        }
    }
}

struct Stat {
    kind: DepKind,
    node_counter: u64,
    edge_counter: u64,
}

struct LocalEncoderState {
    next_node_index: u32,
    remaining_node_index: u32,
    encoder: MemEncoder,
    node_count: usize,
    edge_count: usize,
    seek_offsets: Vec<(DepNodeIndex, usize)>,
    seek_roots: Vec<(DepNode, DepNodeIndex)>,

    /// Stores the number of times we've encoded each dep kind.
    kind_stats: Vec<u32>,
}

struct LocalEncoderResult {
    node_max: u32,
    node_count: usize,
    edge_count: usize,

    /// Stores the number of times we've encoded each dep kind.
    kind_stats: Vec<u32>,
}

struct EncoderState {
    /// One flag for every reserved index, set when validation can keep the
    /// previous row verbatim. Empty when allocating a compact graph.
    retained_rows: IndexVec<SerializedDepNodeIndex, AtomicBool>,
    next_node_index: AtomicU64,
    previous: Arc<SerializedDepGraph>,
    file: Lock<Option<FileEncoder<'static>>>,
    seek_index: Option<Lock<SeekIndexBuilder>>,
    local: WorkerLocal<RefCell<LocalEncoderState>>,
    stats: Option<Lock<FxHashMap<DepKind, Stat>>>,
}

impl EncoderState {
    fn new(
        encoder: FileEncoder<'static>,
        record_stats: bool,
        previous: Arc<SerializedDepGraph>,
        index_check: bool,
        reuse_rows: bool,
    ) -> Self {
        // Preserving previous indices lets us copy verified rows verbatim.
        // Even replacing every key can fit in two generations of slots when
        // we refill holes. Compact once the graph has shrunk below half full.
        let slot_limit = previous.live_node_count.saturating_mul(2);
        let retain_indices =
            reuse_rows && !record_stats && previous.index_space_len() <= slot_limit;
        let retained_rows = if retain_indices {
            IndexVec::from_fn_n(|_| AtomicBool::new(false), previous.index_space_len())
        } else {
            IndexVec::new()
        };
        Self {
            retained_rows,
            previous,
            next_node_index: AtomicU64::new(0),
            stats: record_stats.then(|| Lock::new(FxHashMap::default())),
            file: Lock::new(Some(encoder)),
            seek_index: index_check.then(|| Lock::new(SeekIndexBuilder::default())),
            local: WorkerLocal::new(|_| {
                RefCell::new(LocalEncoderState {
                    next_node_index: 0,
                    remaining_node_index: 0,
                    edge_count: 0,
                    node_count: 0,
                    seek_offsets: Vec::new(),
                    seek_roots: Vec::new(),
                    encoder: MemEncoder::new(),
                    kind_stats: iter::repeat_n(0, DepKind::NUM_VARIANTS).collect(),
                })
            }),
        }
    }

    #[inline]
    fn next_index(&self, local: &mut LocalEncoderState) -> DepNodeIndex {
        loop {
            let index = self.batch_index(local);
            // Slots before the old end are either the two singleton nodes
            // (installed before reservation) or candidates for reclamation.
            if index.as_usize() >= self.retained_rows.len()
                || index.as_u32() <= DepNodeIndex::FOREVER_RED_NODE.as_u32()
                || self.previous.nodes[SerializedDepNodeIndex::from_u32(index.as_u32())].kind
                    == DepKind::Null
            {
                return index;
            }
            local.next_node_index += 1;
            local.remaining_node_index -= 1;
        }
    }

    #[inline]
    fn batch_index(&self, local: &mut LocalEncoderState) -> DepNodeIndex {
        if local.remaining_node_index == 0 {
            const COUNT: u32 = 256;

            // We assume that there won't be enough active threads to overflow `u64` from `u32::MAX` here.
            // This can exceed u32::MAX by at most `N` * `COUNT` where `N` is the thread pool count since
            // `try_into().unwrap()` will make threads panic when `self.next_node_index` exceeds u32::MAX.
            local.next_node_index =
                self.next_node_index.fetch_add(COUNT as u64, Ordering::Relaxed).try_into().unwrap();

            // Check that we'll stay within `u32`
            local.next_node_index.checked_add(COUNT).unwrap();

            local.remaining_node_index = COUNT;
        }

        DepNodeIndex::from_u32(local.next_node_index)
    }

    /// Marks the index previously returned by `next_index` as used.
    #[inline]
    fn bump_index(&self, local: &mut LocalEncoderState) {
        local.remaining_node_index -= 1;
        local.next_node_index += 1;
        local.node_count += 1;
    }

    #[inline]
    fn record(
        &self,
        node: &DepNode,
        index: DepNodeIndex,
        edge_count: usize,
        edges: &[DepNodeIndex],
        retained_graph: &Option<Lock<RetainedDepGraph>>,
        local: &mut LocalEncoderState,
    ) {
        local.kind_stats[node.kind.as_usize()] += 1;
        local.edge_count += edge_count;

        if let Some(retained_graph) = &retained_graph {
            // Outline the build of the full dep graph as it's typically disabled and cold.
            outline(move || {
                // Block on the lock rather than using `try_lock`: under the parallel frontend
                // several threads record nodes concurrently, and dropping a node on lock
                // contention would make the retained graph nondeterministic. Readers take a
                // clone of the graph (`retained_dep_graph`) rather than holding the lock, so
                // this never deadlocks against a reentrant `record`.
                retained_graph.lock().push(index, *node, edges);
            });
        }

        if let Some(stats) = &self.stats {
            let kind = node.kind;

            // Outline the stats code as it's typically disabled and cold.
            outline(move || {
                let mut stats = stats.lock();
                let stat =
                    stats.entry(kind).or_insert(Stat { kind, node_counter: 0, edge_counter: 0 });
                stat.node_counter += 1;
                stat.edge_counter += edge_count as u64;
            });
        }
    }

    #[inline]
    fn flush_mem_encoder(&self, local: &mut LocalEncoderState) {
        let mut file = self.file.lock();
        let encoder = file.as_mut().unwrap();
        let start = encoder.position();
        encoder.emit_raw_bytes(&local.encoder.data);
        if let Some(index) = &self.seek_index {
            // New nodes precede the worker's next index. Re-executed nodes
            // can also occupy any of the previous session's reserved slots.
            let slots = self.retained_rows.len().max(local.next_node_index as usize);
            index.lock().record_chunk(start, slots, &mut local.seek_offsets, &mut local.seek_roots);
        }
        local.encoder.data.clear();
    }

    /// Encodes a node to the current graph.
    fn encode_node(
        &self,
        index: DepNodeIndex,
        node: &NodeInfo<'_>,
        retained_graph: &Option<Lock<RetainedDepGraph>>,
        local: &mut LocalEncoderState,
    ) {
        if self.seek_index.is_some() {
            local.seek_offsets.push((index, local.encoder.data.len()));
            if matches!(node.node.kind, DepKind::typeck_root | DepKind::mir_borrowck) {
                local.seek_roots.push((node.node, index));
            }
        }
        node.encode(&mut local.encoder, index);
        if local.encoder.data.len() > 64 * 1024 {
            self.flush_mem_encoder(&mut *local);
        }
        self.record(&node.node, index, node.edges.len(), node.edges, retained_graph, &mut *local);
    }

    /// Encodes a node that was promoted from the previous graph, reading the node and its
    /// fingerprint directly from the previous dep graph. It expects all edges to already
    /// have a new dep node index assigned.
    #[inline]
    fn encode_promoted_node(
        &self,
        index: DepNodeIndex,
        prev_index: SerializedDepNodeIndex,
        retained_graph: &Option<Lock<RetainedDepGraph>>,
        local: &mut LocalEncoderState,
        edges: &[DepNodeIndex],
    ) {
        let node = NodeInfo {
            node: *self.previous.index_to_node(prev_index),
            value_fingerprint: self.previous.value_fingerprint_for_index(prev_index),
            edges,
        };
        self.encode_node(index, &node, retained_graph, local);
    }

    fn finish(&self, profiler: &SelfProfilerRef, current: &CurrentDepGraph) -> FileEncodeResult {
        // Prevent more indices from being allocated.
        self.next_node_index.store(u32::MAX as u64 + 1, Ordering::SeqCst);

        let results = broadcast(|_| {
            let mut local = self.local.borrow_mut();

            // Prevent more indices from being allocated on this thread.
            local.remaining_node_index = 0;

            self.flush_mem_encoder(&mut *local);

            LocalEncoderResult {
                kind_stats: local.kind_stats.clone(),
                node_max: local.next_node_index,
                node_count: local.node_count,
                edge_count: local.edge_count,
            }
        });

        let mut encoder = self.file.lock().take().unwrap();

        let mut kind_stats: Vec<u32> = iter::repeat_n(0, DepKind::NUM_VARIANTS).collect();

        let mut node_max = 0;
        let mut node_count = 0;
        let mut edge_count = 0;

        for result in results {
            node_max = max(node_max, result.node_max);
            node_count += result.node_count;
            edge_count += result.edge_count;
            for (i, stat) in result.kind_stats.iter().enumerate() {
                kind_stats[i] += stat;
            }
        }

        if !self.retained_rows.is_empty() {
            let (nodes, edges) = self.copy_retained_rows(&mut encoder, &mut kind_stats);
            node_max = node_max.max(self.retained_rows.len() as u32);
            node_count += nodes;
            edge_count += edges;
        }

        // Encode the number of each dep kind encountered
        for count in kind_stats.iter() {
            count.encode(&mut encoder);
        }

        self.previous.session_count.checked_add(1).unwrap().encode(&mut encoder);

        if let Some(index) = &self.seek_index {
            let start = encoder.position();
            index.lock().encode(&mut encoder, start, node_max);
        }

        debug!(?node_max, ?node_count, ?edge_count);
        debug!("position: {:?}", encoder.position());
        IntEncodedWithFixedSize(node_max.try_into().unwrap()).encode(&mut encoder);
        IntEncodedWithFixedSize(node_count.try_into().unwrap()).encode(&mut encoder);
        IntEncodedWithFixedSize(edge_count.try_into().unwrap()).encode(&mut encoder);
        debug!("position: {:?}", encoder.position());
        // Drop the encoder so that nothing is written after the counts.
        let result = encoder.finish();
        if let Ok(position) = result {
            // FIXME(rylev): we hardcode the dep graph file name so we
            // don't need a dependency on rustc_incremental just for that.
            profiler.artifact_size("dep_graph", "dep-graph.bin", position as u64);
        }

        self.print_incremental_info(current, node_count, edge_count);

        result
    }

    /// Append each consecutive run of retained rows with one write. By this
    /// point every worker has flushed its new and re-executed nodes, including
    /// queries whose result stayed green after their dependencies changed.
    fn copy_retained_rows(
        &self,
        encoder: &mut FileEncoder<'_>,
        kinds: &mut [u32],
    ) -> (usize, usize) {
        let mut seek = self.seek_index.as_ref().map(|s| s.lock());
        if let Some(seek) = &mut seek {
            seek.reserve_slots(self.retained_rows.len());
        }
        let bytes = self.previous.row_bytes();
        let (mut offset, mut run_start) = (0, 0);
        let (mut nodes, mut edges) = (0, 0);
        for _ in 0..self.previous.live_node_count {
            let start = offset;
            let header = self.previous.header_at(start);
            let index = header.index();
            let row = self.previous.row_index[index];
            offset = row.end();
            if !self.retained_rows[index].load(Ordering::Relaxed) {
                if run_start != start {
                    encoder.emit_raw_bytes(&bytes[run_start..start]);
                }
                run_start = offset;
                continue;
            }
            let node = header.node();
            nodes += 1;
            edges += row.num_edges as usize;
            kinds[node.kind.as_usize()] += 1;
            if let Some(seek) = &mut seek {
                let position = encoder.position() + (start - run_start);
                seek.record_node(node, DepNodeIndex::from_u32(index.as_u32()), position);
            }
        }
        encoder.emit_raw_bytes(&bytes[run_start..offset]);
        (nodes, edges)
    }

    fn print_incremental_info(
        &self,
        current: &CurrentDepGraph,
        total_node_count: usize,
        total_edge_count: usize,
    ) {
        if let Some(record_stats) = &self.stats {
            let record_stats = record_stats.lock();
            // `stats` is sorted below so we can allow this lint here.
            #[allow(rustc::potential_query_instability)]
            let mut stats: Vec<_> = record_stats.values().collect();
            stats.sort_by_key(|s| -(s.node_counter as i64));

            const SEPARATOR: &str = "[incremental] --------------------------------\
                                     ----------------------------------------------\
                                     ------------";

            eprintln!("[incremental]");
            eprintln!("[incremental] DepGraph Statistics");
            eprintln!("{SEPARATOR}");
            eprintln!("[incremental]");
            eprintln!("[incremental] Total Node Count: {}", total_node_count);
            eprintln!("[incremental] Total Edge Count: {}", total_edge_count);

            if cfg!(debug_assertions) {
                let total_read_count = current.total_read_count.load(Ordering::Relaxed);
                let total_duplicate_read_count =
                    current.total_duplicate_read_count.load(Ordering::Relaxed);
                eprintln!("[incremental] Total Edge Reads: {total_read_count}");
                eprintln!("[incremental] Total Duplicate Edge Reads: {total_duplicate_read_count}");
            }

            eprintln!("[incremental]");
            eprintln!(
                "[incremental]  {:<36}| {:<17}| {:<12}| {:<17}|",
                "Node Kind", "Node Frequency", "Node Count", "Avg. Edge Count"
            );
            eprintln!("{SEPARATOR}");

            for stat in stats {
                let node_kind_ratio =
                    (100.0 * (stat.node_counter as f64)) / (total_node_count as f64);
                let node_kind_avg_edges = (stat.edge_counter as f64) / (stat.node_counter as f64);

                eprintln!(
                    "[incremental]  {:<36}|{:>16.1}% |{:>12} |{:>17.1} |",
                    format!("{:?}", stat.kind),
                    node_kind_ratio,
                    stat.node_counter,
                    node_kind_avg_edges,
                );
            }

            eprintln!("{SEPARATOR}");
            eprintln!("[incremental]");
        }
    }
}

pub(crate) struct GraphEncoder {
    profiler: SelfProfilerRef,
    status: EncoderState,
    /// In-memory copy of the dep graph; only present if `-Zquery-dep-graph` is set.
    retained_graph: Option<Lock<RetainedDepGraph>>,
}

impl GraphEncoder {
    pub(crate) fn new(
        sess: &Session,
        encoder: FileEncoder<'static>,
        prev_index_space_len: usize,
        previous: Arc<SerializedDepGraph>,
    ) -> Self {
        let retained_graph = sess
            .opts
            .unstable_opts
            .query_dep_graph
            .then(|| Lock::new(RetainedDepGraph::new(prev_index_space_len)));
        let index_check =
            sess.opts.unstable_opts.ohm_shared_stages && !sess.opts.output_types.should_codegen();
        let status = EncoderState::new(
            encoder,
            sess.opts.unstable_opts.incremental_info,
            previous,
            index_check,
            retained_graph.is_none(),
        );
        GraphEncoder { status, retained_graph, profiler: sess.prof.clone() }
    }

    #[inline]
    pub(super) fn retains_indices(&self) -> bool {
        !self.status.retained_rows.is_empty()
    }

    /// Called after allocating the two singleton nodes and before any
    /// parallel work. When over a fifth of the slots are vacant, give
    /// each worker a disjoint range to fill before growing the index space.
    pub(super) fn reserve_previous_indices(&self) {
        self.status.local.borrow_mut().remaining_node_index = 0;
        let slots = self.status.retained_rows.len();
        let live = self.status.previous.live_node_count;
        let start = if slots > live + live / 4 {
            u64::from(DepNodeIndex::FOREVER_RED_NODE.as_u32()) + 1
        } else {
            slots as u64
        };
        self.status.next_node_index.store(start, Ordering::Relaxed);
    }

    #[inline]
    pub(super) fn retain_row(
        &self,
        prev_index: SerializedDepNodeIndex,
        colors: &DepNodeColorMap,
    ) -> Option<DepNodeIndex> {
        let index = DepNodeIndex::from_u32(prev_index.as_u32());
        match colors.try_set_color(prev_index, DesiredColor::Green { index }) {
            TrySetColorResult::Success => {
                self.status.retained_rows[prev_index].store(true, Ordering::Relaxed);
                Some(index)
            }
            TrySetColorResult::AlreadyGreen { index } => Some(index),
            TrySetColorResult::AlreadyRed => None,
        }
    }

    pub(crate) fn retained_dep_graph(&self) -> Option<RetainedDepGraph> {
        self.retained_graph.as_ref().map(|retained_graph| retained_graph.lock().clone())
    }

    /// Encodes a node that does not exists in the previous graph.
    pub(crate) fn send_new(
        &self,
        node: DepNode,
        value_fingerprint: Fingerprint,
        edges: &[DepNodeIndex],
    ) -> DepNodeIndex {
        let _prof_timer = self.profiler.generic_activity("incr_comp_encode_dep_graph");
        let node = NodeInfo { node, value_fingerprint, edges };
        let mut local = self.status.local.borrow_mut();
        let index = self.status.next_index(&mut *local);
        self.status.bump_index(&mut *local);
        self.status.encode_node(index, &node, &self.retained_graph, &mut *local);
        index
    }

    /// Encodes a node that exists in the previous graph, but was re-executed.
    /// When retaining indices, keep its slot so parents can reuse their packed
    /// edges even if this node had to execute to prove its value unchanged.
    ///
    /// This will also ensure the dep node is colored either red or green.
    pub(crate) fn send_and_color(
        &self,
        prev_index: SerializedDepNodeIndex,
        colors: &DepNodeColorMap,
        node: DepNode,
        value_fingerprint: Fingerprint,
        edges: &[DepNodeIndex],
        is_green: bool,
    ) -> DepNodeIndex {
        let _prof_timer = self.profiler.generic_activity("incr_comp_encode_dep_graph");
        let node = NodeInfo { node, value_fingerprint, edges };

        let mut local = self.status.local.borrow_mut();

        let index = if self.retains_indices() {
            DepNodeIndex::from_u32(prev_index.as_u32())
        } else {
            self.status.next_index(&mut *local)
        };
        let color = if is_green { DesiredColor::Green { index } } else { DesiredColor::Red };

        // Use `try_set_color` to avoid racing when `send_promoted` is called concurrently
        // on the same index.
        match colors.try_set_color(prev_index, color) {
            TrySetColorResult::Success => {}
            TrySetColorResult::AlreadyRed => panic!("dep node {prev_index:?} is unexpectedly red"),
            TrySetColorResult::AlreadyGreen { index } => return index,
        }

        if self.retains_indices() {
            local.node_count += 1;
        } else {
            self.status.bump_index(&mut *local);
        }
        self.status.encode_node(index, &node, &self.retained_graph, &mut *local);
        index
    }

    /// Encodes a node that was promoted from the previous graph. It reads the information directly
    /// from the previous dep graph and expects all edges to already have a new dep node index
    /// assigned.
    ///
    /// Tries to mark the dep node green, and returns Some if it is now green,
    /// or None if had already been concurrently marked red.
    #[inline]
    pub(crate) fn send_promoted(
        &self,
        prev_index: SerializedDepNodeIndex,
        colors: &DepNodeColorMap,
        edges: &[DepNodeIndex],
    ) -> Option<DepNodeIndex> {
        let _prof_timer = self.profiler.generic_activity("incr_comp_encode_dep_graph");

        let mut local = self.status.local.borrow_mut();
        let index = self.status.next_index(&mut *local);

        // Use `try_set_color` to avoid racing when `send_promoted` or `send_and_color`
        // is called concurrently on the same index.
        match colors.try_set_color(prev_index, DesiredColor::Green { index }) {
            TrySetColorResult::Success => {
                self.status.bump_index(&mut *local);
                self.status.encode_promoted_node(
                    index,
                    prev_index,
                    &self.retained_graph,
                    &mut *local,
                    edges,
                );
                Some(index)
            }
            TrySetColorResult::AlreadyRed => None,
            TrySetColorResult::AlreadyGreen { index } => Some(index),
        }
    }

    pub(crate) fn finish(&self, current: &CurrentDepGraph) -> FileEncodeResult {
        let _prof_timer = self.profiler.generic_activity("incr_comp_encode_dep_graph_finish");

        self.status.finish(&self.profiler, current)
    }
}
