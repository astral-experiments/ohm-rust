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
use rustc_errors::Diag;
use rustc_parse::lexer::StripTokens;
use rustc_parse::{exp, load_source_file, new_parser_from_source_file, unwrap_or_emit_fatal};
use rustc_serialize::opaque::MemDecoder;
use rustc_serialize::opaque::mem_encoder::MemEncoder;
use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};
use rustc_session::Session;
use rustc_session::parse::ModuleParseEffects;
use rustc_span::edition::Edition;
use rustc_span::{BytePos, Ident, SourceFile, SourceFileHash, Span, SyntaxContext, sym};
use thin_vec::ThinVec;

use crate::syntax_codec::{
    DecodeSpans, EncodeSpans, SYNTAX_CACHE, SyntaxCacheKey, SyntaxCacheOp, SyntaxDecoder,
    SyntaxEncoder,
};

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
    (*SYNTAX_CACHE)(
        SyntaxCacheKey::Module(file.stable_id),
        SyntaxCacheOp::Load(&mut |bytes| {
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
        (*SYNTAX_CACHE)(SyntaxCacheKey::Module(file.stable_id), SyntaxCacheOp::Save(bytes));
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
    let attr_base = sess.psess.attr_id_generator.reserve_ids(nattrs);
    let mut read = SyntaxDecoder::new(data, &mut table, attr_base, |_| Some(start))?;
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
    let mut encoder = SyntaxEncoder::new(data, attrs, file, snapshot_limit(file));
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
    encoder.finish(table_pos)
}

impl EncodeSpans for &SourceFile {
    fn encode(&mut self, data: &mut MemEncoder, span: Span) -> bool {
        let span = span.data_untracked();
        if !span.ctxt.is_root() || span.parent.is_some() {
            return false;
        }
        if span.is_dummy() {
            data.emit_raw_bytes(&u32::MAX.to_le_bytes());
        } else if self.contains(span.lo) && self.contains(span.hi) {
            let start = (span.lo - self.start_pos).0;
            data.emit_raw_bytes(&start.to_le_bytes());
            data.emit_u32((span.hi - span.lo).0);
        } else {
            return false;
        }
        true
    }
}

impl DecodeSpans for BytePos {
    #[inline]
    fn decode(&self, data: &mut MemDecoder<'_>) -> Span {
        let off = u32::from_le_bytes(data.read_raw_bytes(4).try_into().unwrap());
        if off == u32::MAX {
            return rustc_span::DUMMY_SP;
        }
        let lo = *self + BytePos(off);
        Span::new(lo, lo + BytePos(data.read_u32()), SyntaxContext::root(), None)
    }
}
