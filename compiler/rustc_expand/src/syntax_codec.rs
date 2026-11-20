//! Shared format for syntax snapshots. The caller supplies the span encoding.
//! Each snapshot has one symbol dictionary and a contiguous attribute ID range.

use rustc_data_structures::fx::FxIndexSet;
use rustc_serialize::opaque::mem_encoder::MemEncoder;
use rustc_serialize::opaque::{MAGIC_END_BYTES, MemDecoder};
use rustc_serialize::{Decoder, Encoder};
use rustc_span::def_id::{CrateNum, DefId, DefIndex, DefPathHash, LocalDefId};
use rustc_span::{
    AttrId, BlobDecoder, ByteSymbol, ExpnId, Span, SpanDecoder, SpanEncoder, Symbol, SyntaxContext,
};

#[derive(Clone, Copy)]
pub enum SyntaxCacheKey {
    Module(rustc_span::StableSourceFileId),
    Macro((rustc_span::StableSourceFileId, CrateNum)),
}

/// rustc_expand_queries provides the query-backed implementation.
pub enum SyntaxCacheOp<'a> {
    Load(&'a mut dyn FnMut(&[u8])),
    Save(Vec<u8>),
}

type SyntaxCacheHook = fn(SyntaxCacheKey, SyntaxCacheOp<'_>);
pub static SYNTAX_CACHE: rustc_data_structures::AtomicRef<SyntaxCacheHook> =
    rustc_data_structures::AtomicRef::new(&((|_, _| {}) as SyntaxCacheHook));

pub(crate) trait EncodeSpans {
    fn encode(&mut self, data: &mut MemEncoder, span: Span) -> bool;
    fn finish(self, _data: &mut MemEncoder)
    where
        Self: Sized,
    {
    }
}

pub(crate) trait DecodeSpans {
    fn decode(&self, data: &mut MemDecoder<'_>) -> Span;
}

impl<S: EncodeSpans> SyntaxEncoder<S> {
    pub(crate) fn new(
        data: MemEncoder,
        attrs: std::ops::Range<u32>,
        spans: S,
        limit: usize,
    ) -> Self {
        SyntaxEncoder {
            data,
            symbols: FxIndexSet::default(),
            attrs,
            spans,
            limit,
            token_depth: 0,
            valid: true,
        }
    }

    pub(crate) fn finish(self, table_pos: usize) -> Option<Vec<u8>> {
        if !self.valid {
            return None;
        }
        let mut data = self.data;
        let symbols_pos = u32::try_from(data.position()).ok()?;
        data.data[table_pos..table_pos + 4].copy_from_slice(&symbols_pos.to_le_bytes());
        write_symbols(&mut data, self.symbols);
        self.spans.finish(&mut data);
        data.emit_raw_bytes(MAGIC_END_BYTES);
        (data.position() <= self.limit).then(|| data.finish())
    }
}

impl<'a, S> SyntaxDecoder<'a, S> {
    pub(crate) fn new(
        data: MemDecoder<'a>,
        table: &mut MemDecoder<'a>,
        attr_base: u32,
        spans: impl FnOnce(&mut MemDecoder<'a>) -> Option<S>,
    ) -> Option<Self> {
        let symbols = read_symbols(table);
        Some(SyntaxDecoder { data, symbols, attr_base, spans: spans(table)? })
    }
}

pub(crate) struct SyntaxEncoder<S> {
    data: MemEncoder,
    pub(crate) symbols: FxIndexSet<Symbol>,
    attrs: std::ops::Range<u32>,
    pub(crate) spans: S,
    limit: usize,
    token_depth: usize,
    valid: bool,
}

pub(crate) struct SyntaxDecoder<'a, S> {
    data: MemDecoder<'a>,
    pub(crate) symbols: Vec<Symbol>,
    spans: S,
    pub(crate) attr_base: u32,
}

// Forward primitive operations to the ordinary in-memory codec.
macro_rules! codecs {
    ($($ty:ty => $emit:ident, $read:ident;)*) => {
        impl<S: EncodeSpans> Encoder for SyntaxEncoder<S> {
            fn emit_raw_bytes(&mut self, bytes: &[u8]) { self.data.emit_raw_bytes(bytes); }
            fn emit_u32(&mut self, value: u32) {
                self.data.emit_raw_bytes(&value.to_le_bytes());
            }
            fn emit_usize(&mut self, value: usize) {
                self.valid &= value <= u16::MAX as usize;
                self.data.emit_raw_bytes(&(value as u16).to_le_bytes());
            }
            $(#[inline] fn $emit(&mut self, value: $ty) { self.data.$emit(value); })*
        }
        impl<S: DecodeSpans> Encoder for Compare<'_, '_, S> {
            fn emit_raw_bytes(&mut self, bytes: &[u8]) {
                self.equal = self.equal && self.decoder.read_raw_bytes(bytes.len()) == bytes;
            }
            fn emit_u32(&mut self, value: u32) {
                self.equal = self.equal && self.decoder.read_u32() == value;
            }
            fn emit_usize(&mut self, value: usize) {
                self.equal = self.equal && self.decoder.read_usize() == value;
            }
            $(#[inline] fn $emit(&mut self, value: $ty) {
                self.equal = self.equal && self.decoder.$read() == value;
            })*
        }
        impl<S: DecodeSpans> Decoder for SyntaxDecoder<'_, S> {
            #[inline]
            fn read_raw_bytes(&mut self, n: usize) -> &[u8] { self.data.read_raw_bytes(n) }
            fn position(&self) -> usize { self.data.position() }
            fn peek_byte(&self) -> u8 { self.data.peek_byte() }
            #[inline]
            fn read_u32(&mut self) -> u32 {
                u32::from_le_bytes(self.data.read_raw_bytes(4).try_into().unwrap())
            }
            #[inline]
            fn read_usize(&mut self) -> usize {
                u16::from_le_bytes(self.data.read_raw_bytes(2).try_into().unwrap()) as usize
            }
            $(#[inline] fn $read(&mut self) -> $ty { self.data.$read() })*
        }
    }
}
codecs! {
    u8 => emit_u8, read_u8;
    u16 => emit_u16, read_u16;

    u64 => emit_u64, read_u64;
    u128 => emit_u128, read_u128;

    i16 => emit_i16, read_i16;
    i32 => emit_i32, read_i32;
    i64 => emit_i64, read_i64;
    i128 => emit_i128, read_i128;
    isize => emit_isize, read_isize;
}

/// An AST fragment can refer only to its recorded spans and attributes.
/// Reject a snapshot if it contains a reference requiring any other table.
impl<S: EncodeSpans> SpanEncoder for SyntaxEncoder<S> {
    fn encode_token_stream(&mut self, encode: impl FnOnce(&mut Self)) {
        // A transcriber can duplicate an Arc-backed subtree at every level.
        // Bound both recursion depth and the size of its flattened encoding.
        self.valid &= self.token_depth < 256 && self.data.position() <= self.limit;
        if self.valid {
            self.token_depth += 1;
            encode(self);
            self.token_depth -= 1;
        }
    }

    fn should_encode_attr_tokens(&mut self) -> bool {
        self.valid &= self.data.position() <= self.limit;
        self.valid
    }

    fn encode_attr_id(&mut self, id: AttrId) {
        if self.attrs.contains(&id.as_u32()) {
            self.emit_u32(id.as_u32() - self.attrs.start);
        } else {
            self.valid = false;
        }
    }

    fn encode_span(&mut self, span: Span) {
        self.valid &= self.spans.encode(&mut self.data, span);
    }
    fn encode_symbol(&mut self, symbol: Symbol) {
        let idx = self.symbols.insert_full(symbol).0;
        self.emit_u32(idx as u32);
    }
    fn encode_byte_symbol(&mut self, symbol: ByteSymbol) {
        self.emit_byte_str(symbol.as_byte_str());
    }
    fn encode_expn_id(&mut self, _: ExpnId) {
        self.valid = false;
    }
    fn encode_syntax_context(&mut self, _: SyntaxContext) {
        self.valid = false;
    }
    fn encode_def_index(&mut self, _: DefIndex) {
        self.valid = false;
    }
    fn encode_crate_num(&mut self, _: CrateNum) {
        self.valid = false;
    }
    fn encode_def_id(&mut self, _: DefId) {
        self.valid = false;
    }
}

impl<S: DecodeSpans> BlobDecoder for SyntaxDecoder<'_, S> {
    #[inline]
    fn decode_symbol(&mut self) -> Symbol {
        let idx = self.read_u32();
        self.symbols[idx as usize]
    }
    fn decode_byte_symbol(&mut self) -> ByteSymbol {
        ByteSymbol::intern(self.read_byte_str())
    }
    fn decode_def_index(&mut self) -> DefIndex {
        unreachable!()
    }
}
impl<S: DecodeSpans> SpanDecoder for SyntaxDecoder<'_, S> {
    #[inline]
    fn decode_span(&mut self) -> Span {
        self.spans.decode(&mut self.data)
    }
    fn decode_attr_id(&mut self) -> AttrId {
        AttrId::from_u32(self.attr_base + self.read_u32())
    }
    fn decode_expn_id(&mut self) -> ExpnId {
        unreachable!()
    }
    fn decode_syntax_context(&mut self) -> SyntaxContext {
        unreachable!()
    }
    fn decode_crate_num(&mut self) -> CrateNum {
        unreachable!()
    }
    fn decode_def_id(&mut self) -> DefId {
        unreachable!()
    }
}

// Compare a current token stream directly with its encoding. This compares all
// fields, including the original positions inside NtIdent and NtLifetime,
// where Ident's ordinary equality compares only symbol and context.
struct Compare<'a, 'b, S> {
    decoder: &'a mut SyntaxDecoder<'b, S>,
    equal: bool,
}
impl<S: DecodeSpans> SyntaxDecoder<'_, S> {
    pub(crate) fn matches(&mut self, input: &rustc_ast::tokenstream::TokenStream) -> bool {
        use rustc_serialize::Encodable;
        let mut compare = Compare { decoder: self, equal: true };
        input.encode(&mut compare);
        compare.equal
    }
}
impl<S: DecodeSpans> SpanEncoder for Compare<'_, '_, S> {
    fn encode_token_stream(&mut self, encode: impl FnOnce(&mut Self)) {
        if self.equal {
            encode(self);
        }
    }

    fn encode_span(&mut self, span: Span) {
        self.equal = self.equal && self.decoder.decode_span() == span;
    }
    fn encode_symbol(&mut self, symbol: Symbol) {
        self.equal = self.equal && self.decoder.decode_symbol() == symbol;
    }
    fn encode_byte_symbol(&mut self, symbol: ByteSymbol) {
        self.equal = self.equal && self.decoder.decode_byte_symbol() == symbol;
    }
    fn encode_attr_id(&mut self, _: AttrId) {
        self.equal = false;
    }
    fn encode_expn_id(&mut self, _: ExpnId) {
        self.equal = false;
    }
    fn encode_syntax_context(&mut self, _: SyntaxContext) {
        self.equal = false;
    }
    fn encode_def_index(&mut self, _: DefIndex) {
        self.equal = false;
    }
    fn encode_def_id(&mut self, _: DefId) {
        self.equal = false;
    }
    fn encode_crate_num(&mut self, _: CrateNum) {
        self.equal = false;
    }
}

type ParentHash = fn(LocalDefId) -> Option<DefPathHash>;
type ParentId = fn(DefPathHash) -> Option<LocalDefId>;
pub static PARENT_HASH: rustc_data_structures::AtomicRef<ParentHash> =
    rustc_data_structures::AtomicRef::new(&((|_| None) as _));
pub static PARENT_ID: rustc_data_structures::AtomicRef<ParentId> =
    rustc_data_structures::AtomicRef::new(&((|_| None) as _));

pub(crate) fn write_symbols(data: &mut MemEncoder, symbols: FxIndexSet<Symbol>) {
    data.emit_usize(symbols.len());
    for symbol in symbols {
        if Symbol::is_predefined(symbol.as_u32()) {
            data.emit_u32(symbol.as_u32() + 1);
        } else {
            data.emit_u32(0);
            data.emit_str(symbol.as_str());
        }
    }
}
pub(crate) fn read_symbols(table: &mut MemDecoder<'_>) -> Vec<Symbol> {
    let count = table.read_usize();
    (0..count)
        .map(|_| match table.read_u32() {
            0 => Symbol::intern(table.read_str()),
            index => Symbol::new(index - 1),
        })
        .collect()
}
