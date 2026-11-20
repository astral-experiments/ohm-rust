//! Contains `ParseSess` which holds state living beyond what one `Parser` might.
//! It also serves as an input to the parser itself.

use std::sync::Arc;

use rustc_ast::attr::AttrIdGenerator;
use rustc_ast::node_id::NodeId;
use rustc_data_structures::fx::{FxHashMap, FxHashSet, FxIndexMap};
use rustc_data_structures::sync::{AppendOnlyVec, DynSend, DynSync, Lock};
use rustc_errors::annotate_snippet_emitter_writer::AnnotateSnippetEmitter;
use rustc_errors::emitter::{EmitterWithNote, stderr_destination};
use rustc_errors::{
    BufferedEarlyLint, ColorConfig, DecorateDiagCompat, Diag, DiagCtxt, DiagCtxtHandle, Level,
    MultiSpan,
};
use rustc_span::edition::Edition;
use rustc_span::hygiene::ExpnId;
use rustc_span::source_map::{FilePathMapping, SourceMap};
use rustc_span::{Span, Symbol};

use crate::Session;
use crate::lint::{Lint, LintId};

/// Collected spans during parsing for places where a certain feature was
/// used and should be feature gated accordingly in `check_crate` in `rustc_ast_passes`.
#[derive(Default)]
pub struct GatedSpans {
    pub spans: Lock<FxHashMap<Symbol, Vec<Span>>>,
}

impl GatedSpans {
    /// Feature gate the given `span` under the given `feature`
    /// which is same `Symbol` used in `unstable.rs`.
    pub fn gate(&self, feature: Symbol, span: Span) {
        record(|log| log.effects.gates.push((feature, span)));
        self.spans.borrow_mut().entry(feature).or_default().push(span);
    }

    /// Ungate the last span under the given `feature`.
    /// Panics if the given `span` wasn't the last one.
    ///
    /// Using this is discouraged unless you have a really good reason to.
    pub fn ungate_last(&self, feature: Symbol, span: Span) {
        record(|log| {
            if let Some(i) = log.effects.gates.iter().rposition(|entry| *entry == (feature, span)) {
                log.effects.gates.remove(i);
            } else {
                log.unsupported = true;
            }
        });
        let removed_span = self.spans.borrow_mut().entry(feature).or_default().pop().unwrap();
        debug_assert_eq!(span, removed_span);
    }

    /// Prepend the given set of `spans` onto the set in `self`.
    pub fn merge(&self, mut spans: FxHashMap<Symbol, Vec<Span>>) {
        let mut inner = self.spans.borrow_mut();
        // The entries will be moved to another map so the drain order does not
        // matter.
        #[allow(rustc::potential_query_instability)]
        for (gate, mut gate_spans) in inner.drain() {
            spans.entry(gate).or_default().append(&mut gate_spans);
        }
        *inner = spans;
    }
}

#[derive(Default)]
pub struct SymbolGallery {
    /// All symbols occurred and their first occurrence span.
    pub symbols: Lock<FxIndexMap<Symbol, Span>>,
}

impl SymbolGallery {
    /// Insert a symbol and its span into symbol gallery.
    /// If the symbol has occurred before, ignore the new occurrence.
    pub fn insert(&self, symbol: Symbol, span: Span) {
        record(|log| {
            if log.seen.insert(symbol) {
                log.effects.symbols.push((symbol, span));
            }
        });
        self.symbols.lock().entry(symbol).or_insert(span);
    }
}

/// Info about a parsing session.
pub struct ParseSess {
    dcx: DiagCtxt,
    pub edition: Edition,
    /// Places where raw identifiers were used. This is used to avoid complaining about idents
    /// clashing with keywords in new editions.
    pub raw_identifier_spans: AppendOnlyVec<Span>,
    /// Places where identifiers that contain invalid Unicode codepoints but that look like they
    /// should be. Useful to avoid bad tokenization when encountering emoji. We group them to
    /// provide a single error per unique incorrect identifier.
    pub bad_unicode_identifiers: Lock<FxIndexMap<Symbol, Vec<Span>>>,
    source_map: Arc<SourceMap>,
    pub buffered_lints: Lock<Vec<BufferedEarlyLint>>,
    /// Contains the spans of block expressions that could have been incomplete based on the
    /// operation token that followed it, but that the parser cannot identify without further
    /// analysis.
    pub ambiguous_block_expr_parse: Lock<FxIndexMap<Span, Span>>,
    pub gated_spans: GatedSpans,
    pub symbol_gallery: SymbolGallery,
    /// Used to generate new `AttrId`s. Every `AttrId` is unique.
    pub attr_id_generator: AttrIdGenerator,
}

impl ParseSess {
    /// Used for testing.
    pub fn new() -> Self {
        let sm = Arc::new(SourceMap::new(FilePathMapping::empty()));
        let emitter = Box::new(
            AnnotateSnippetEmitter::new(stderr_destination(ColorConfig::Auto))
                .sm(Some(Arc::clone(&sm))),
        );
        let dcx = DiagCtxt::new(emitter);
        ParseSess::with_dcx(dcx, sm)
    }

    pub fn with_dcx(dcx: DiagCtxt, source_map: Arc<SourceMap>) -> Self {
        Self {
            dcx,
            edition: ExpnId::root().expn_data().edition,
            raw_identifier_spans: Default::default(),
            bad_unicode_identifiers: Lock::new(Default::default()),
            source_map,
            buffered_lints: Lock::new(vec![]),
            ambiguous_block_expr_parse: Lock::new(Default::default()),
            gated_spans: GatedSpans::default(),
            symbol_gallery: SymbolGallery::default(),
            attr_id_generator: AttrIdGenerator::new(),
        }
    }

    pub fn emitter_with_note(note: String) -> Self {
        let sm = Arc::new(SourceMap::new(FilePathMapping::empty()));
        let emitter = Box::new(AnnotateSnippetEmitter::new(stderr_destination(ColorConfig::Auto)));
        let dcx = DiagCtxt::new(Box::new(EmitterWithNote { emitter, note }));
        ParseSess::with_dcx(dcx, sm)
    }

    #[inline]
    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    pub fn clone_source_map(&self) -> Arc<SourceMap> {
        Arc::clone(&self.source_map)
    }

    pub fn buffer_lint(
        &self,
        lint: &'static Lint,
        span: impl Into<MultiSpan>,
        node_id: NodeId,
        diagnostic: impl Into<DecorateDiagCompat>,
    ) {
        self.opt_span_buffer_lint(lint, Some(span.into()), node_id, diagnostic.into())
    }

    pub fn dyn_buffer_lint<
        F: for<'a> FnOnce(DiagCtxtHandle<'a>, Level) -> Diag<'a> + DynSync + DynSend + 'static,
    >(
        &self,
        lint: &'static Lint,
        span: impl Into<MultiSpan>,
        node_id: NodeId,
        callback: F,
    ) {
        self.opt_span_buffer_lint(
            lint,
            Some(span.into()),
            node_id,
            DecorateDiagCompat(Box::new(|dcx, level, _| callback(dcx, level))),
        )
    }

    pub fn dyn_buffer_lint_sess<
        F: for<'a> FnOnce(DiagCtxtHandle<'a>, Level, &Session) -> Diag<'a>
            + DynSync
            + DynSend
            + 'static,
    >(
        &self,
        lint: &'static Lint,
        span: impl Into<MultiSpan>,
        node_id: NodeId,
        callback: F,
    ) {
        self.opt_span_buffer_lint(
            lint,
            Some(span.into()),
            node_id,
            DecorateDiagCompat(Box::new(|dcx, level, sess| {
                let sess = sess.downcast_ref::<Session>().expect("expected a `Session`");
                callback(dcx, level, sess)
            })),
        )
    }

    pub(crate) fn opt_span_buffer_lint(
        &self,
        lint: &'static Lint,
        span: Option<MultiSpan>,
        node_id: NodeId,
        diagnostic: DecorateDiagCompat,
    ) {
        self.buffered_lints.with_lock(|buffered_lints| {
            buffered_lints.push(BufferedEarlyLint {
                span,
                node_id,
                lint_id: LintId::of(lint),
                diagnostic,
            });
        });
    }

    pub fn dcx(&self) -> DiagCtxtHandle<'_> {
        self.dcx.handle()
    }
}

/// Mutations that a successful source-file parse leaves in the shared parse session.
#[derive(Default, rustc_macros::Encodable, rustc_macros::Decodable)]
pub struct ModuleParseEffects {
    symbols: Vec<(Symbol, Span)>,
    raw_identifiers: Vec<Span>,
    ambiguous: Vec<(Span, Span)>,
    gates: Vec<(Symbol, Span)>,
}

#[derive(Default)]
struct Recorder {
    effects: ModuleParseEffects,
    seen: FxHashSet<Symbol>,
    unsupported: bool,
}

thread_local! {
    static MODULE_PARSE: std::cell::RefCell<Option<Recorder>> = const { std::cell::RefCell::new(None) };
}

#[inline]
fn record(effect: impl FnOnce(&mut Recorder)) {
    MODULE_PARSE.with_borrow_mut(|slot| {
        if let Some(log) = slot {
            effect(log)
        }
    })
}

impl ParseSess {
    pub fn record_raw_identifier(&self, span: Span) {
        record(|log| log.effects.raw_identifiers.push(span));
        self.raw_identifier_spans.push(span);
    }

    pub fn record_ambiguous_block(&self, span: Span, lhs: Span) {
        record(|log| log.effects.ambiguous.push((span, lhs)));
        self.ambiguous_block_expr_parse.borrow_mut().insert(span, lhs);
    }
}

impl ModuleParseEffects {
    /// Record successful, diagnostic-free parses. Buffered lints and bad
    /// identifiers can produce diagnostics later under different lint levels.
    pub fn capture<T>(sess: &ParseSess, parse: impl FnOnce() -> T) -> (T, Option<Self>) {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                MODULE_PARSE.take();
            }
        }
        let _reset = Reset;
        let lints = sess.buffered_lints.borrow().len();
        let before = sess.dcx().emission_stamp();
        MODULE_PARSE.replace(Some(Recorder::default()));
        let result = parse();
        if before != sess.dcx().emission_stamp()
            || !sess.bad_unicode_identifiers.borrow().is_empty()
            || sess.buffered_lints.borrow().len() != lints
        {
            return (result, None);
        }
        (
            result,
            MODULE_PARSE
                .take()
                .filter(|recorder| !recorder.unsupported)
                .map(|recorder| recorder.effects),
        )
    }

    pub fn replay(self, sess: &ParseSess) {
        {
            let mut gallery = sess.symbol_gallery.symbols.lock();
            for (symbol, span) in self.symbols {
                gallery.entry(symbol).or_insert(span);
            }
        }
        for span in self.raw_identifiers {
            sess.raw_identifier_spans.push(span);
        }
        sess.ambiguous_block_expr_parse.borrow_mut().extend(self.ambiguous);
        for (feature, span) in self.gates {
            sess.gated_spans.gate(feature, span);
        }
    }
}
