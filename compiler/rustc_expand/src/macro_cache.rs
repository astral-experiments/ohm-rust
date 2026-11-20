//! Save the parsed result of one invocation, before collecting nested macros.
//! Name resolution, recursive expansion and diagnostics on the resulting tree
//! still run in the current session. Only imported rules are eligible: a query
//! dependency on their defining crate invalidates changes to the rule set.

use std::hash::Hasher;
use std::mem;
use std::ops::Range;

use rustc_ast::ast;
use rustc_ast::tokenstream::TokenStream;
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::fx::{FxHashMap, FxIndexMap, FxIndexSet};
use rustc_data_structures::stable_hash::StableHasher;
use rustc_serialize::opaque::mem_encoder::MemEncoder;
use rustc_serialize::opaque::{MAGIC_END_BYTES, MemDecoder};
use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};
use rustc_session::Session;
use rustc_session::parse::ModuleParseEffects;
use rustc_span::def_id::{CrateNum, DefPathHash, LocalDefId};
use rustc_span::edition::Edition;
use rustc_span::hygiene::{ExpnHash, Transparency};
use rustc_span::source_map::SourceMap;
use rustc_span::symbol::Symbol;
use rustc_span::{
    BytePos, ExpnId, MetavarAccess, MetavarSpansMap, SourceFileHash, Span, StableSourceFileId,
    SyntaxContext,
};

use crate::base::{ExpandResult, ExtCtxt};
use crate::expand::{AstFragment, AstFragmentKind};
use crate::syntax_codec::{
    DecodeSpans, EncodeSpans, PARENT_HASH, PARENT_ID, SYNTAX_CACHE, SyntaxCacheKey, SyntaxCacheOp,
    SyntaxDecoder, SyntaxEncoder,
};

const MAX_SNAPSHOT: usize = 1024 * 1024;

// One query and symbol dictionary for calls rooted in the same file
// and using rules from the same dependency. Source and dependency changes
// refresh the batch. A miss inside a green batch expands normally.
type Key = (StableSourceFileId, CrateNum);
#[derive(Default)]
pub(crate) struct Cache(FxIndexMap<Key, Batch>);

enum Batch {
    Read {
        symbols: Vec<Symbol>,
        entries: FxHashMap<u64, Vec<u8>>,
        accesses: Vec<(Span, MetavarAccess)>,
    },
    Write {
        symbols: FxIndexSet<Symbol>,
        entries: Vec<(u64, Vec<u8>)>,
        len: usize,
    },
}

impl Cache {
    pub(crate) fn flush(&mut self) {
        for (key, batch) in mem::take(&mut self.0) {
            if let Batch::Write { symbols, entries, .. } = batch
                && !entries.is_empty()
            {
                let mut data = MemEncoder::new();
                crate::syntax_codec::write_symbols(&mut data, symbols);
                entries.encode(&mut data);
                data.emit_raw_bytes(MAGIC_END_BYTES);
                (*SYNTAX_CACHE)(SyntaxCacheKey::Macro(key), SyntaxCacheOp::Save(data.finish()));
            }
        }
    }
}
impl Batch {
    fn get(key: Key) -> Self {
        let mut batch = None;
        (*SYNTAX_CACHE)(
            SyntaxCacheKey::Macro(key),
            SyntaxCacheOp::Load(&mut |bytes| {
                if let Ok(mut data) = MemDecoder::new(bytes, 0) {
                    let symbols = crate::syntax_codec::read_symbols(&mut data);
                    let entries =
                        (0..data.read_usize()).map(|_| Decodable::decode(&mut data)).collect();
                    batch = Some(Batch::Read { symbols, entries, accesses: Vec::new() });
                }
            }),
        );
        batch.unwrap_or_else(|| Batch::Write {
            symbols: FxIndexSet::default(),
            entries: Vec::new(),
            len: 0,
        })
    }
}

/// The public API hash tracks the semantics of the imported macro. Also
/// compare its actual definition including each token's source coordinates:
/// forwarding to a procedural macro can expose positions and spacing.
pub(crate) fn definition_stamp(
    sess: &Session,
    def: &ast::MacroDef,
    transparency: Transparency,
    edition: Edition,
) -> Option<Fingerprint> {
    if sess.opts.incremental.is_none()
        || !sess.opts.unstable_opts.ohm_shared_stages
        || sess.opts.unstable_opts.macro_stats
    {
        return None;
    }
    let mut data = MemEncoder::new();
    data.emit_raw_bytes(&[0; 4]);
    let spans = MacroSpans {
        sm: sess.source_map(),
        files: FxIndexMap::default(),
        contexts: FxIndexSet::default(),
    };
    let mut encoder = SyntaxEncoder::new(data, 0..0, spans, MAX_SNAPSHOT);
    def.encode(&mut encoder);
    transparency.encode(&mut encoder);
    (edition as u8).encode(&mut encoder);
    let mut hasher = StableHasher::new();
    hasher.write(&encoder.finish(0)?);
    Some(hasher.finish())
}

pub(crate) struct Invocation<'a> {
    pub sess: &'a Session,
    pub id: ExpnId,
    pub stamp: Fingerprint,
    pub kind: AstFragmentKind,
    pub site: Span,
    pub input: &'a TokenStream,
}

impl Invocation<'_> {
    pub(crate) fn expand<'s>(
        &self,
        cx: &mut ExtCtxt<'s>,
        expand: impl FnOnce(&mut ExtCtxt<'s>) -> ExpandResult<Option<AstFragment>, ()>,
    ) -> ExpandResult<Option<AstFragment>, ()> {
        if !matches!(
            self.kind,
            AstFragmentKind::Expr
                | AstFragmentKind::MethodReceiverExpr
                | AstFragmentKind::OptExpr
                | AstFragmentKind::Stmts
        ) {
            return expand(cx);
        }
        let Some(def) = self.id.expn_data().macro_def_id.filter(|d| !d.is_local()) else {
            return expand(cx);
        };
        let file = self.sess.source_map().lookup_source_file(self.site.source_callsite().lo());
        let key = (file.stable_id, def.krate);
        let hash = self.id.expn_hash().local_hash().as_u64();
        let batch = cx.macro_cache.0.entry(key).or_insert_with(|| Batch::get(key));
        match batch {
            Batch::Read { symbols, entries, accesses } => {
                if let Some(bytes) = entries.get(&hash)
                    && let Some(saved) = self.load(bytes, symbols, accesses)
                {
                    tracing::info!(target: "ohm_macro_syntax", "reused macro expansion");
                    return ExpandResult::Ready(Some(saved));
                }
                return expand(cx);
            }
            Batch::Write { len, .. } if *len >= 16 * MAX_SNAPSHOT => return expand(cx),
            Batch::Write { .. } => {}
        }
        let psess = &self.sess.psess;
        let first_attr = psess.attr_id_generator.reserve_ids(0);
        let ((fragment, effects), accesses) =
            MetavarSpansMap::capture(|| ModuleParseEffects::capture(psess, || expand(cx)));
        let end_attr = psess.attr_id_generator.reserve_ids(0);
        if let ExpandResult::Ready(Some(ref value)) = fragment
            && let Some(accesses) = accesses
            && let Some(effects) = effects
        {
            if let Some(Batch::Write { symbols, entries, len }) = cx.macro_cache.0.get_mut(&key)
                && let Some(bytes) =
                    self.snapshot(symbols, first_attr..end_attr, value, &effects, &accesses)
            {
                *len += bytes.len();
                entries.push((hash, bytes));
            }
        }
        fragment
    }

    fn snapshot(
        &self,
        symbols: &mut FxIndexSet<Symbol>,
        attrs: Range<u32>,
        fragment: &AstFragment,
        effects: &ModuleParseEffects,
        accesses: &FxIndexMap<Span, MetavarAccess>,
    ) -> Option<Vec<u8>> {
        let mut data = MemEncoder::new();
        self.stamp.encode(&mut data);
        data.emit_u8(self.kind as u8);
        data.emit_u32(attrs.end - attrs.start);
        let table_pos = data.position();
        data.emit_raw_bytes(&[0; 4]);
        let spans = Pooled {
            map: FxIndexSet::default(),
            descriptors: MemEncoder::new(),
            inner: MacroSpans {
                sm: self.sess.source_map(),
                files: FxIndexMap::default(),
                contexts: FxIndexSet::default(),
            },
        };
        let mut encoder = SyntaxEncoder::new(data, attrs, spans, MAX_SNAPSHOT);
        let symbol_count = symbols.len();
        mem::swap(symbols, &mut encoder.symbols);
        self.site.encode(&mut encoder);
        self.input.encode(&mut encoder);
        accesses.encode(&mut encoder);
        effects.encode(&mut encoder);
        match fragment {
            AstFragment::Expr(expr) | AstFragment::MethodReceiverExpr(expr) => {
                expr.encode(&mut encoder)
            }
            AstFragment::OptExpr(expr) => expr.encode(&mut encoder),
            AstFragment::Stmts(stmts) => stmts.as_slice().encode(&mut encoder),
            _ => unreachable!(),
        }
        mem::swap(symbols, &mut encoder.symbols);
        let bytes = encoder.finish(table_pos);
        if bytes.is_none() {
            symbols.truncate(symbol_count);
        }
        bytes
    }

    fn load(
        &self,
        bytes: &[u8],
        symbols: &mut Vec<Symbol>,
        accesses: &mut Vec<(Span, MetavarAccess)>,
    ) -> Option<AstFragment> {
        let mut data = MemDecoder::new(bytes, 0).ok()?;
        if Fingerprint::decode(&mut data) != self.stamp || data.read_u8() != self.kind as u8 {
            return None;
        }
        let attrs = data.read_u32();
        let table_pos = u32::from_le_bytes(data.read_raw_bytes(4).try_into().unwrap()) as usize;
        let mut table = data.split_at(table_pos);
        let mut read = SyntaxDecoder::new(data, &mut table, 0, |data| {
            PooledReader::load(self.sess.source_map(), data)
        })?;
        mem::swap(symbols, &mut read.symbols);
        let result = (|| {
            if self.site != Span::decode(&mut read) || !read.matches(self.input) {
                return None;
            }
            let count = read.read_usize();
            accesses.clear();
            accesses.extend((0..count).map(|_| <(Span, MetavarAccess)>::decode(&mut read)));
            if !rustc_span::with_metavar_spans(|map| map.replay(accesses)) {
                return None;
            }
            read.attr_base = self.sess.psess.attr_id_generator.reserve_ids(attrs);
            let effects = ModuleParseEffects::decode(&mut read);
            let fragment = match self.kind {
                AstFragmentKind::Expr => AstFragment::Expr(Decodable::decode(&mut read)),
                AstFragmentKind::MethodReceiverExpr => {
                    AstFragment::MethodReceiverExpr(Decodable::decode(&mut read))
                }
                AstFragmentKind::OptExpr => AstFragment::OptExpr(Decodable::decode(&mut read)),
                AstFragmentKind::Stmts => {
                    AstFragment::Stmts(Vec::<ast::Stmt>::decode(&mut read).into())
                }
                _ => unreachable!(),
            };
            assert_eq!(read.position(), table_pos);
            effects.replay(&self.sess.psess);
            Some(fragment)
        })();
        mem::swap(symbols, &mut read.symbols);
        result
    }
}

struct MacroSpans<'a> {
    sm: &'a SourceMap,
    files: FxIndexMap<StableSourceFileId, (SourceFileHash, BytePos, bool)>,
    contexts: FxIndexSet<(SyntaxContext, Option<LocalDefId>)>,
}

impl EncodeSpans for MacroSpans<'_> {
    fn encode(&mut self, data: &mut MemEncoder, span: Span) -> bool {
        let span = span.data_untracked();

        let context = self.contexts.insert_full((span.ctxt, span.parent)).0;
        let (file_index, offset) = if span.is_dummy() {
            (0, 0)
        } else {
            let file = self.sm.lookup_source_file(span.lo);
            if !file.contains(span.hi) {
                return false;
            }
            let idx = self
                .files
                .insert_full(file.stable_id, (file.src_hash, file.start_pos, file.is_imported()))
                .0;
            (idx + 1, (span.lo - file.start_pos).0)
        };
        let len = (span.hi - span.lo).0;
        if context > u8::MAX as usize || file_index > u8::MAX as usize || len > u16::MAX as u32 {
            return false;
        }
        let packed = (offset as u64)
            | ((len as u64) << 32)
            | ((context as u64) << 48)
            | ((file_index as u64) << 56);
        data.emit_raw_bytes(&packed.to_le_bytes());
        true
    }
    fn finish(self, data: &mut MemEncoder) {
        data.emit_usize(self.files.len());
        let mut positions = self.files.values().map(|(_, start, _)| *start).collect::<Vec<_>>();
        positions.sort_unstable();
        for (id, (hash, start, imported)) in self.files {
            id.encode(data);
            hash.encode(data);
            imported.encode(data);
            data.emit_u32(positions.binary_search(&start).unwrap() as u32);
        }
        data.emit_usize(self.contexts.len());
        for (ctxt, parent) in self.contexts {
            parent.map(|p| (*PARENT_HASH)(p)).encode(data);
            let marks = ctxt.marks();
            data.emit_usize(marks.len());
            for (expn, t) in marks {
                expn.expn_hash().encode(data);
                t.encode(data);
            }
        }
    }
}

struct MappedSpans {
    files: Vec<BytePos>,
    contexts: Vec<(SyntaxContext, Option<LocalDefId>)>,
}
impl MappedSpans {
    fn load(sm: &SourceMap, data: &mut MemDecoder<'_>) -> Option<Self> {
        let nfiles = data.read_usize();
        let mut files = Vec::with_capacity(nfiles + 1);
        files.push(BytePos(0));
        let mut order = Vec::with_capacity(nfiles);
        for _ in 0..nfiles {
            let file = sm.source_file_by_stable_id(StableSourceFileId::decode(data))?;
            if file.src_hash != SourceFileHash::decode(data)
                || file.is_imported() != bool::decode(data)
            {
                return None;
            }
            files.push(file.start_pos);
            order.push((data.read_u32(), file.start_pos));
        }
        order.sort_unstable();
        if !order.iter().map(|(_, pos)| pos).is_sorted() {
            return None;
        }
        let ncontexts = data.read_usize();
        let mut contexts = Vec::with_capacity(ncontexts);
        for _ in 0..ncontexts {
            let parent = if let Some(hash) = Option::<Option<DefPathHash>>::decode(data) {
                Some((*PARENT_ID)(hash?)?)
            } else {
                None
            };
            let mut ctxt = SyntaxContext::root();
            for _ in 0..data.read_usize() {
                let expn = ExpnId::from_hash(ExpnHash::decode(data))?;
                let t = Transparency::decode(data);
                ctxt = SyntaxContext::from_key((ctxt, expn, t));
            }
            contexts.push((ctxt, parent));
        }
        Some(Self { files, contexts })
    }
}
impl DecodeSpans for MappedSpans {
    #[inline]
    fn decode(&self, data: &mut MemDecoder<'_>) -> Span {
        let packed = u64::from_le_bytes(data.read_raw_bytes(8).try_into().unwrap());
        let (ctxt, parent) = self.contexts[(packed >> 48) as u8 as usize];
        let lo = self.files[(packed >> 56) as usize] + BytePos(packed as u32);
        let hi = lo + BytePos((packed >> 32) as u16 as u32);
        Span::new(lo, hi, ctxt, parent)
    }
}

struct Pooled<'a> {
    map: FxIndexSet<Span>,
    descriptors: MemEncoder,
    inner: MacroSpans<'a>,
}
impl EncodeSpans for Pooled<'_> {
    fn encode(&mut self, data: &mut MemEncoder, span: Span) -> bool {
        let (index, new) = self.map.insert_full(span);
        if index > u16::MAX as usize {
            return false;
        }
        data.emit_raw_bytes(&(index as u16).to_le_bytes());
        !new || self.inner.encode(&mut self.descriptors, span)
    }
    fn finish(self, data: &mut MemEncoder) {
        self.inner.finish(data);
        data.emit_usize(self.map.len());
        data.emit_raw_bytes(&self.descriptors.data);
    }
}
struct PooledReader(Vec<Span>);
impl PooledReader {
    fn load(sm: &SourceMap, data: &mut MemDecoder<'_>) -> Option<Self> {
        let inner = MappedSpans::load(sm, data)?;
        let spans = (0..data.read_usize()).map(|_| inner.decode(data)).collect();
        Some(PooledReader(spans))
    }
}
impl DecodeSpans for PooledReader {
    #[inline]
    fn decode(&self, data: &mut MemDecoder<'_>) -> Span {
        self.0[u16::from_le_bytes(data.read_raw_bytes(2).try_into().unwrap()) as usize]
    }
}
