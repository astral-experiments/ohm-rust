//! Reuse the parser's input to expansion for an unchanged out-of-line module.
//!
//! Cache the parser output before expansion resolves any names or applies
//! user macros. Source spans are relative to this file, and attribute IDs
//! are relative to the range allocated by this parse. On a hit, restore
//! those ranges and the lexer/parser's contributions to the session.
//!
//! The query depends on the source hash. Also check the inputs stored in
//! the payload because a module declaration can acquire attributes without
//! editing the file, and multiple declarations can load the same file.

use std::path::Path;

use rustc_ast::visit::Visitor;
use rustc_ast::{AttrKind, AttrVec, Attribute, Item, ItemKind, ModKind, ModSpans};
use rustc_data_structures::fx::FxIndexSet;
use rustc_errors::Diag;
use rustc_parse::lexer::StripTokens;
use rustc_parse::{exp, load_source_file, new_parser_from_source_file, unwrap_or_emit_fatal};
use rustc_serialize::opaque::mem_encoder::MemEncoder;
use rustc_serialize::opaque::{MAGIC_END_BYTES, MemDecoder};
use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};
use rustc_session::Session;
use rustc_session::parse::ModuleParseEffects;
use rustc_span::def_id::{CrateNum, DefId, DefIndex};
use rustc_span::edition::Edition;
use rustc_span::{
    AttrId, BlobDecoder, BytePos, ByteSymbol, ExpnId, Ident, SourceFile, SourceFileHash, Span,
    SpanDecoder, SpanEncoder, Symbol, SyntaxContext, sym,
};
use thin_vec::ThinVec;

/// rustc_expand_queries provides the query-backed implementation.
pub enum ModuleCacheOp<'a> {
    Load(&'a mut dyn FnMut(&[u8])),
    Save(Vec<u8>),
}

type ModuleCacheHook = fn(rustc_span::StableSourceFileId, ModuleCacheOp<'_>);
pub static MODULE_CACHE: rustc_data_structures::AtomicRef<ModuleCacheHook> =
    rustc_data_structures::AtomicRef::new(&((|_, _| {}) as ModuleCacheHook));

// Amortize the per-file query and the symbol dictionary.
const MIN_FILE_BYTES: u32 = 1024;

// Token streams of nested annotated items can overlap. Bound the amount
// materialized by the source size, checking before each lazy stream.
fn snapshot_limit(file: &SourceFile) -> usize {
    (file.normalized_source_len.0 as usize).saturating_mul(16)
}

type Module = (AttrVec, ThinVec<Box<Item>>, ModSpans);

#[derive(Clone, Copy, PartialEq, rustc_macros::Encodable, rustc_macros::Decodable)]
struct Inputs {
    source: SourceFileHash,
    edition: u8,
    keyword_lint_inert: bool,
    test_bodies_inert: bool,
}

pub(super) fn parse<'s>(
    sess: &'s Session,
    path: &Path,
    span: Span,
    keyword_lint_only: bool,
    outer_docs_only: bool,
    lint_items: &mut Option<Vec<u32>>,
) -> Result<Module, Diag<'s>> {
    let file = load_source_file(&sess.psess, path, Some(span));
    let parse = || {
        let mut parser = unwrap_or_emit_fatal(new_parser_from_source_file(
            &sess.psess,
            file.clone(),
            StripTokens::ShebangAndFrontmatter,
        ));
        parser.parse_mod(exp!(Eof))
    };
    if sess.opts.incremental.is_none()
        || !sess.opts.unstable_opts.ohm_shared_stages
        || file.normalized_source_len.0 < MIN_FILE_BYTES
    {
        return parse();
    }
    let keyword_lint_inert = keyword_lint_only && sess.edition() == Edition::Edition2024;
    let inputs = Inputs {
        source: file.src_hash,
        edition: sess.edition() as u8,
        keyword_lint_inert,
        test_bodies_inert: keyword_lint_inert
            && outer_docs_only
            && !sess.config.contains(&(sym::test, None)),
    };
    let mut saved = None;
    (*MODULE_CACHE)(
        file.stable_id,
        ModuleCacheOp::Load(&mut |bytes| {
            saved = load(sess, file.start_pos, inputs, bytes);
        }),
    );
    if let Some((module, selected)) = saved {
        tracing::info!(target: "ohm_module_syntax", "reused parsed module {}", path.display());
        *lint_items = selected;
        return Ok(module);
    }

    let first_attr = sess.psess.attr_id_generator.reserve_ids(0);
    let (module, effects) = ModuleParseEffects::capture(&sess.psess, parse);
    let mut module = module?;
    let next_attr = sess.psess.attr_id_generator.reserve_ids(0);
    if let Some(effects) = effects
        && let Some(bytes) = snapshot(&file, inputs, first_attr..next_attr, &mut module, &effects)
    {
        (*MODULE_CACHE)(file.stable_id, ModuleCacheOp::Save(bytes));
    }
    Ok(module)
}

fn load(
    sess: &Session,
    start: BytePos,
    inputs: Inputs,
    bytes: &[u8],
) -> Option<(Module, Option<Vec<u32>>)> {
    let mut data = MemDecoder::new(bytes, 0).ok()?;
    if Inputs::decode(&mut data) != inputs {
        return None;
    }
    let nattrs = data.read_u32();
    let symbols_pos = u32::from_le_bytes(data.read_raw_bytes(4).try_into().unwrap()) as usize;
    let mut table = data.split_at(symbols_pos);
    let count = table.read_usize();
    let symbols = (0..count)
        .map(|_| match table.read_u32() {
            0 => Symbol::intern(table.read_str()),
            index => Symbol::new(index - 1),
        })
        .collect();
    let attr_base = sess.psess.attr_id_generator.reserve_ids(nattrs);
    let mut read = ModuleDecoder { data, start, symbols, attr_base };
    let effects = ModuleParseEffects::decode(&mut read);
    let selected = Option::<Vec<u32>>::decode(&mut read);
    let module = Module::decode(&mut read);
    assert_eq!(read.position(), symbols_pos);
    effects.replay(&sess.psess);
    Some((module, selected))
}

/// The built-in pre-expansion pass checks keywords from older editions.
/// Its lint-level builder also visits explicit lint attributes.
struct NeedsLintVisit(bool);

impl<'a> Visitor<'a> for NeedsLintVisit {
    fn visit_attribute(&mut self, attr: &'a Attribute) {
        self.0 |= rustc_lint_defs::Level::from_opt_symbol(attr.name()).is_some();
        rustc_ast::visit::walk_attribute(self, attr);
    }
    fn visit_ident(&mut self, id: &'a Ident) {
        self.0 |= !id.span.ctxt().is_root();
    }
}

fn has_only_cfg_test(item: &Item) -> bool {
    let [attr] = &item.attrs[..] else { return false };
    attr.has_name(sym::cfg)
        && attr.meta_item_list().is_some_and(
            |args| matches!(&args[..], [name] if name.is_word() && name.has_name(sym::test)),
        )
}

/// Encode the syntax and effects first, then the per-file symbol dictionary.
fn snapshot(
    file: &SourceFile,
    inputs: Inputs,
    attrs: std::ops::Range<u32>,
    module: &mut Module,
    effects: &ModuleParseEffects,
) -> Option<Vec<u8>> {
    let mut data = MemEncoder::new();
    inputs.encode(&mut data);
    data.emit_u32(attrs.end - attrs.start);
    let table_pos = data.position();
    data.emit_raw_bytes(&[0; 4]);
    let mut encoder =
        ModuleEncoder { data, symbols: FxIndexSet::default(), attrs, file, valid: true };
    effects.encode(&mut encoder);
    let mut omitted = Vec::new();
    let selected = inputs.keyword_lint_inert.then(|| {
        // An attribute on either side of `mod name;` could expose the
        // tokens of the entire module to a procedural macro. Only prune
        // when both sets of attributes consist solely of doc comments.
        let elide_tests = inputs.test_bodies_inert
            && module.0.iter().all(|a| matches!(a.kind, AttrKind::DocComment(..)));
        let mut indices = Vec::new();
        for (i, item) in module.1.iter_mut().enumerate() {
            let mut needs_lint = NeedsLintVisit(false);
            needs_lint.visit_item(item);
            if needs_lint.0 {
                indices.push(i as u32);
            } else if elide_tests
                && has_only_cfg_test(item)
                && let ItemKind::Mod(_, _, ModKind::Loaded(ref mut items, ..)) = item.kind
            {
                // The parser already checked the entire file, including this
                // body. Keep the declaration so cfg processing (including
                // check-cfg) still runs in the new session.
                omitted.push((i, std::mem::take(items), item.tokens.take()));
            }
        }
        indices
    });
    selected.encode(&mut encoder);
    module.encode(&mut encoder);
    // The regular expansion sees the original tree on a miss.
    for (i, children, tokens) in omitted {
        let item = &mut module.1[i];
        let ItemKind::Mod(_, _, ModKind::Loaded(ref mut items, ..)) = item.kind else {
            unreachable!()
        };
        *items = children;
        item.tokens = tokens;
    }
    if !encoder.valid {
        return None;
    }
    let mut data = encoder.data;
    let symbols_pos = u32::try_from(data.position()).ok()?;
    data.data[table_pos..table_pos + 4].copy_from_slice(&symbols_pos.to_le_bytes());
    data.emit_usize(encoder.symbols.len());
    for symbol in encoder.symbols {
        if Symbol::is_predefined(symbol.as_u32()) {
            data.emit_u32(symbol.as_u32() + 1);
        } else {
            data.emit_u32(0);
            data.emit_str(symbol.as_str());
        }
    }
    data.emit_raw_bytes(MAGIC_END_BYTES);
    (data.position() <= snapshot_limit(file)).then(|| data.finish())
}

struct ModuleEncoder<'a> {
    data: MemEncoder,
    symbols: FxIndexSet<Symbol>,
    attrs: std::ops::Range<u32>,
    file: &'a SourceFile,
    valid: bool,
}

struct ModuleDecoder<'a> {
    data: MemDecoder<'a>,
    symbols: Vec<Symbol>,
    start: BytePos,
    attr_base: u32,
}

// Forward primitive operations to the ordinary in-memory codec.
macro_rules! codecs {
    ($($ty:ty => $emit:ident, $read:ident;)*) => {
        impl Encoder for ModuleEncoder<'_> {
            fn emit_raw_bytes(&mut self, bytes: &[u8]) { self.data.emit_raw_bytes(bytes); }
            $(#[inline] fn $emit(&mut self, value: $ty) { self.data.$emit(value); })*
        }
        impl Decoder for ModuleDecoder<'_> {
            #[inline] fn read_raw_bytes(&mut self, n: usize) -> &[u8] { self.data.read_raw_bytes(n) }
            fn position(&self) -> usize { self.data.position() }
            fn peek_byte(&self) -> u8 { self.data.peek_byte() }
            $(#[inline] fn $read(&mut self) -> $ty { self.data.$read() })*
        }
    }
}
codecs! {
    u8 => emit_u8, read_u8;
    u16 => emit_u16, read_u16;
    u32 => emit_u32, read_u32;
    u64 => emit_u64, read_u64;
    u128 => emit_u128, read_u128;
    usize => emit_usize, read_usize;
    i16 => emit_i16, read_i16;
    i32 => emit_i32, read_i32;
    i64 => emit_i64, read_i64;
    i128 => emit_i128, read_i128;
    isize => emit_isize, read_isize;
}

/// A parsed source file can refer only to its own spans and attributes.
/// Reject a snapshot if it contains a reference requiring any other table.
impl SpanEncoder for ModuleEncoder<'_> {
    fn should_encode_attr_tokens(&mut self) -> bool {
        self.valid &= self.data.position() <= snapshot_limit(self.file);
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
        let span = span.data_untracked();
        if !span.ctxt.is_root() || span.parent.is_some() {
            self.valid = false;
            return;
        }
        if span.is_dummy() {
            self.emit_raw_bytes(&u32::MAX.to_le_bytes());
        } else if self.file.contains(span.lo) && self.file.contains(span.hi) {
            let start = (span.lo - self.file.start_pos).0;
            self.emit_raw_bytes(&start.to_le_bytes());
            self.emit_u32((span.hi - span.lo).0);
        } else {
            self.valid = false;
        }
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

impl BlobDecoder for ModuleDecoder<'_> {
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
impl SpanDecoder for ModuleDecoder<'_> {
    #[inline]
    fn decode_span(&mut self) -> Span {
        let off = u32::from_le_bytes(self.read_raw_bytes(4).try_into().unwrap());
        if off == u32::MAX {
            return rustc_span::DUMMY_SP;
        }
        let lo = self.start + BytePos(off);
        Span::new(lo, lo + BytePos(self.read_u32()), SyntaxContext::root(), None)
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
