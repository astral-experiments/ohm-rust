//! Compare fresh expansion with the retained program, excluding only the editable bodies.

use std::hash::{Hash, Hasher};

use rustc_ast::mut_visit::{self, MutVisitor};
use rustc_ast::{self as ast, ItemKind, NodeId};
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::stable_hash::{StableHash, StableHasher};
use rustc_middle::ich::StableHashState;
use rustc_middle::ty::TyCtxt;
use rustc_serialize::{Encodable, Encoder};
use rustc_span::def_id::{CrateNum, DefId, DefIndex};
use rustc_span::hygiene::{ExpnId, SyntaxContext};
use rustc_span::{BytePos, ByteSymbol, Span, SpanEncoder, Symbol};

use super::ResidentFrontend;

impl ResidentFrontend<'_, '_> {
    /// Hash the complete expanded AST after removing exactly the supported edit inputs.
    /// A fresh compiler repeats expansion on the actual source before producing this proof.
    pub fn expansion_fingerprint(&self, tcx: TyCtxt<'_>) -> Result<Fingerprint, &'static str> {
        if tcx.sess.opts.unstable_opts.incremental_ignore_spans {
            return Err("macro validation requires source spans");
        }
        let body_start = self.source_start + BytePos(self.prefix.len() as u32);
        let mut krate = self.krate.clone();
        struct Normalize {
            function: NodeId,
            module: NodeId,
            constant: Option<NodeId>,
            body_start: BytePos,
        }
        impl MutVisitor for Normalize {
            fn visit_id(&mut self, id: &mut NodeId) {
                // Parser allocation counters can change when the edited body changes size.
                *id = ast::DUMMY_NODE_ID;
            }

            fn visit_item(&mut self, item: &mut ast::Item) {
                if item.id == self.function {
                    let ItemKind::Fn(function) = &mut item.kind else { unreachable!() };
                    function.body = None;
                    item.tokens = None;
                    item.span = item.span.with_hi(self.body_start);
                } else if Some(item.id) == self.constant {
                    let ItemKind::Const(constant) = &mut item.kind else { unreachable!() };
                    constant.body = None;
                    item.tokens = None;
                } else if item.id == self.module {
                    let ItemKind::Mod(_, _, ast::ModKind::Loaded(_, _, spans)) = &mut item.kind
                    else {
                        unreachable!()
                    };
                    spans.inner_span = spans.inner_span.with_hi(self.body_start);
                }
                mut_visit::walk_item(self, item);
            }
        }
        Normalize {
            function: self.item,
            module: self.module,
            constant: self.constant.as_ref().map(|constant| constant.item.id),
            body_start,
        }
        .visit_crate(&mut krate);

        tcx.with_stable_hashing_context(|hcx| {
            let mut encoder = ExpansionEncoder {
                hcx,
                hasher: StableHasher::new(),
                body_start,
                source_end: tcx
                    .sess
                    .source_map()
                    .source_file_by_stable_id(self.source)
                    .ok_or("edited source is missing")?
                    .end_position(),
                overlapping_span: false,
            };
            krate.encode(&mut encoder);
            self.attrs.encode(&mut encoder);
            // Synthetic macro source files affect snippets and diagnostics as well as tokens.
            // Real source bytes outside the edited file must also remain identical.
            let mut files: Vec<_> = tcx
                .sess
                .source_map()
                .files()
                .iter()
                .filter(|file| file.stable_id != self.source)
                .map(|file| (file.stable_id, file.src_hash))
                .collect();
            files.sort_by_key(|(id, _)| *id);
            files.encode(&mut encoder);
            let mut environment: Vec<_> = tcx.sess.env_depinfo.borrow().iter().copied().collect();
            environment.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
            environment.encode(&mut encoder);
            if encoder.overlapping_span {
                return Err("expanded syntax outside the edit refers into the edited body");
            }
            Ok(encoder.hasher.finish())
        })
    }
}

struct ExpansionEncoder<'a> {
    hcx: StableHashState<'a>,
    hasher: StableHasher,
    body_start: BytePos,
    source_end: BytePos,
    overlapping_span: bool,
}

macro_rules! encode_numbers {
    ($($method:ident: $ty:ty),* $(,)?) => {
        $(fn $method(&mut self, value: $ty) { value.hash(&mut self.hasher); })*
    };
}

impl Encoder for ExpansionEncoder<'_> {
    encode_numbers! {
        emit_usize: usize, emit_u128: u128, emit_u64: u64, emit_u32: u32,
        emit_u16: u16, emit_u8: u8, emit_isize: isize, emit_i128: i128,
        emit_i64: i64, emit_i32: i32, emit_i16: i16,
    }

    fn emit_raw_bytes(&mut self, bytes: &[u8]) {
        self.hasher.write(bytes);
    }
}

impl SpanEncoder for ExpansionEncoder<'_> {
    const ENCODE_LAZY_ATTR_TOKEN_STREAM: bool = true;

    fn encode_span(&mut self, span: Span) {
        let data = span.data_untracked();
        self.overlapping_span |=
            data.hi > self.body_start && data.lo <= self.source_end && data.hi <= self.source_end;
        span.stable_hash(&mut self.hcx, &mut self.hasher);
    }

    fn encode_symbol(&mut self, symbol: Symbol) {
        self.emit_str(symbol.as_str());
    }

    fn encode_byte_symbol(&mut self, symbol: ByteSymbol) {
        self.emit_byte_str(symbol.as_byte_str());
    }

    fn encode_expn_id(&mut self, id: ExpnId) {
        id.stable_hash(&mut self.hcx, &mut self.hasher);
    }

    fn encode_syntax_context(&mut self, context: SyntaxContext) {
        context.stable_hash(&mut self.hcx, &mut self.hasher);
    }

    fn encode_crate_num(&mut self, krate: CrateNum) {
        krate.as_def_id().stable_hash(&mut self.hcx, &mut self.hasher);
    }

    fn encode_def_index(&mut self, index: DefIndex) {
        self.emit_u32(index.as_u32());
    }

    fn encode_def_id(&mut self, id: DefId) {
        id.stable_hash(&mut self.hcx, &mut self.hasher);
    }
}
