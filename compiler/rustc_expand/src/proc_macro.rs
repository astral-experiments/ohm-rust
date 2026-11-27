use rustc_ast as ast;
use rustc_ast::tokenstream::TokenStream;
use rustc_data_structures::AtomicRef;
use rustc_data_structures::profiling::TimingGuard;
use rustc_errors::ErrorGuaranteed;
use rustc_parse::parser::{AllowConstBlockItems, ForceCollect, Parser};
use rustc_proc_macro as pm;
use rustc_session::Session;
use rustc_session::config::ProcMacroExecutionStrategy;
use rustc_span::profiling::SpannedEventArgRecorder;
use rustc_span::{FileName, LocalExpnId, SourceFileHash, Span};

use crate::base::{self, *};
use crate::{diagnostics, proc_macro_server};

fn exec_strategy(sess: &Session) -> impl pm::bridge::server::ExecutionStrategy + 'static {
    pm::bridge::server::MaybeCrossThread {
        cross_thread: sess.opts.unstable_opts.proc_macro_execution_strategy
            == ProcMacroExecutionStrategy::CrossThread,
    }
}

fn record_expand_proc_macro<'a>(
    ecx: &ExtCtxt<'a>,
    name: &'static str,
    span: Span,
) -> TimingGuard<'a> {
    ecx.sess.prof.generic_activity_with_arg_recorder(name, |recorder| {
        recorder.record_arg_with_span(ecx.sess.source_map(), ecx.expansion_descr(), span);
    })
}

pub struct BangProcMacro {
    pub client: pm::bridge::client::Client,
}

impl base::BangProcMacro for BangProcMacro {
    fn expand(
        &self,
        ecx: &mut ExtCtxt<'_>,
        span: Span,
        input: TokenStream,
    ) -> Result<TokenStream, ErrorGuaranteed> {
        let _timer = record_expand_proc_macro(ecx, "expand_proc_macro", span);

        let proc_macro_backtrace = ecx.ecfg.proc_macro_backtrace;
        let strategy = exec_strategy(ecx.sess);
        let server = proc_macro_server::Rustc::new(ecx);
        self.client.run1(&strategy, server, input, proc_macro_backtrace).map_err(|e| {
            ecx.dcx().emit_err(diagnostics::ProcMacroPanicked {
                span,
                message: e
                    .into_string()
                    .map(|message| diagnostics::ProcMacroPanickedHelp { message }),
            })
        })
    }
}

pub struct AttrProcMacro {
    pub dylib_hash: Option<SourceFileHash>,
    pub client: pm::bridge::client::Client,
}

impl base::AttrProcMacro for AttrProcMacro {
    fn expand(
        &self,
        ecx: &mut ExtCtxt<'_>,
        span: Span,
        annotation: TokenStream,
        annotated: TokenStream,
    ) -> Result<TokenStream, ErrorGuaranteed> {
        let _timer = record_expand_proc_macro(ecx, "expand_attr_proc_macro_outer", span);

        if let Some(hash) = self.dylib_hash {
            (*EXPAND_ATTRIBUTE_MACRO_CACHED)(hash, annotation, annotated, ecx, self.client)
        } else {
            expand_attribute_macro(span, annotation, annotated, ecx, self.client)
        }
    }
}

pub struct DeriveProcMacro {
    pub dylib_hash: Option<SourceFileHash>,
    pub client: ProcMacroClient,
}

impl MultiItemModifier for DeriveProcMacro {
    fn expand(
        &self,
        ecx: &mut ExtCtxt<'_>,
        span: Span,
        _meta_item: &ast::MetaItem,
        item: Annotatable,
        _is_derive_const: bool,
    ) -> ExpandResult<Vec<Annotatable>, Annotatable> {
        let _timer = record_expand_proc_macro(ecx, "expand_derive_proc_macro_outer", span);

        // We need special handling for statement items
        // (e.g. `fn foo() { #[derive(Debug)] struct Bar; }`)
        let is_stmt = matches!(item, Annotatable::Stmt(..));

        let input = item.to_tokens();

        let invoc_id = ecx.current_expansion.id;

        let res = if let Some(hash) = self.dylib_hash {
            (*EXPAND_DERIVE_MACRO_CACHED)(hash, invoc_id, input, ecx, self.client)
        } else {
            expand_derive_macro(invoc_id, input, ecx, self.client)
        };

        let Ok(output) = res else {
            // error will already have been emitted
            return ExpandResult::Ready(vec![]);
        };

        let error_count_before = ecx.dcx().err_count();
        let mut parser = Parser::new(&ecx.sess.psess, output, Some("proc-macro derive"));
        let mut items = vec![];

        loop {
            match parser.parse_item(
                ForceCollect::No,
                if is_stmt { AllowConstBlockItems::No } else { AllowConstBlockItems::Yes },
            ) {
                Ok(None) => break,
                Ok(Some(item)) => {
                    if is_stmt {
                        items.push(Annotatable::Stmt(Box::new(ecx.stmt_item(span, item))));
                    } else {
                        items.push(Annotatable::Item(item));
                    }
                }
                Err(err) => {
                    err.emit();
                    break;
                }
            }
        }

        // fail if there have been errors emitted
        if ecx.dcx().err_count() > error_count_before {
            ecx.dcx().emit_err(diagnostics::ProcMacroDeriveTokens { span });
        }

        ExpandResult::Ready(items)
    }
}

type ProcMacroClient = pm::bridge::client::Client;

/// Execute the attribute client, retaining the ordinary panic diagnostic on a cache miss.
pub fn expand_attribute_macro(
    span: Span,
    annotation: TokenStream,
    annotated: TokenStream,
    ecx: &mut ExtCtxt<'_>,
    client: ProcMacroClient,
) -> Result<TokenStream, ErrorGuaranteed> {
    let _timer = record_expand_proc_macro(ecx, "expand_proc_macro", span);
    let proc_macro_backtrace = ecx.ecfg.proc_macro_backtrace;
    let strategy = exec_strategy(ecx.sess);
    let server = proc_macro_server::Rustc::new(ecx);
    client.run2(&strategy, server, annotation, annotated, proc_macro_backtrace).map_err(|e| {
        ecx.dcx().emit_err(diagnostics::CustomAttributePanicked {
            span,
            message: e
                .into_string()
                .map(|message| diagnostics::CustomAttributePanickedHelp { message }),
        })
    })
}

pub fn expand_derive_macro(
    invoc_id: LocalExpnId,
    input: TokenStream,
    ecx: &mut ExtCtxt<'_>,
    client: ProcMacroClient,
) -> Result<TokenStream, ()> {
    let _timer =
        ecx.sess.prof.generic_activity_with_arg_recorder("expand_proc_macro", |recorder| {
            let invoc_expn_data = invoc_id.expn_data();
            let span = invoc_expn_data.call_site;
            let event_arg = invoc_expn_data.kind.descr();
            recorder.record_arg_with_span(ecx.sess.source_map(), event_arg, span);
        });

    let proc_macro_backtrace = ecx.ecfg.proc_macro_backtrace;
    let strategy = exec_strategy(ecx.sess);
    let server = proc_macro_server::Rustc::new(ecx);

    match client.run1(&strategy, server, input, proc_macro_backtrace) {
        Ok(stream) => Ok(stream),
        Err(e) => {
            let invoc_expn_data = invoc_id.expn_data();
            let span = invoc_expn_data.call_site;
            ecx.dcx().emit_err({
                diagnostics::ProcMacroDerivePanicked {
                    span,
                    message: e
                        .into_string()
                        .map(|message| diagnostics::ProcMacroDerivePanickedHelp { message }),
                }
            });
            Err(())
        }
    }
}

pub static EXPAND_DERIVE_MACRO_CACHED: AtomicRef<
    fn(
        SourceFileHash,
        LocalExpnId,
        TokenStream,
        &mut ExtCtxt<'_>,
        ProcMacroClient,
    ) -> Result<TokenStream, ()>,
> = AtomicRef::new(
    &(|_, _, _, _: &mut ExtCtxt<'_>, _| -> Result<_, _> {
        panic!(
            "`EXPAND_DERIVE_MACRO_CACHED` callback was not setup; it must be set in `rustc_interface::callbacks`"
        )
    } as _),
);

/// Runs an attribute expansion through the incremental query system.
pub static EXPAND_ATTRIBUTE_MACRO_CACHED: AtomicRef<
    fn(
        SourceFileHash,
        TokenStream,
        TokenStream,
        &mut ExtCtxt<'_>,
        ProcMacroClient,
    ) -> Result<TokenStream, ErrorGuaranteed>,
> = AtomicRef::new(
    &(|_, _, _, _: &mut ExtCtxt<'_>, _| -> Result<_, _> {
        panic!("attribute proc macro cache callback was not set up")
    } as _),
);

pub static INVALIDATE_CACHED_EXPANSION: AtomicRef<fn()> = AtomicRef::new(&((|| {}) as _));

pub(super) fn invalidate_cached_expansion() {
    (*INVALIDATE_CACHED_EXPANSION)();
}

pub static PROC_MACRO_SOURCE_CODE: AtomicRef<fn(&str) -> FileName> =
    AtomicRef::new(&(FileName::proc_macro_source_code as _));

pub(super) fn proc_macro_source_code(src: &str) -> FileName {
    (*PROC_MACRO_SOURCE_CODE)(src)
}

/// Intern the same hygiene contexts on a cache hit as a freshly constructed server.
pub fn prepare_proc_macro_expansion_globals(ecx: &ExtCtxt<'_>) {
    proc_macro_server::Rustc::expansion_globals(ecx, &ecx.current_expansion.id.expn_data());
}

/// Eager expression expansion needs the resolver's dependency-tracking context.
pub static EXPAND_EXPR: AtomicRef<fn(&mut ExtCtxt<'_>, Box<ast::Expr>) -> Box<ast::Expr>> =
    AtomicRef::new(&(expand_expr as _));

pub fn expand_expr(ecx: &mut ExtCtxt<'_>, expr: Box<ast::Expr>) -> Box<ast::Expr> {
    ecx.expander().fully_expand_fragment(crate::expand::AstFragment::Expr(expr)).make_expr()
}
