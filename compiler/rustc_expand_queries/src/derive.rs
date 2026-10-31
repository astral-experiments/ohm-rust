use rustc_ast::tokenstream::TokenStream;
use rustc_expand::base::ExtCtxt;
use rustc_expand::proc_macro::{expand_attribute_macro, expand_derive_macro};
use rustc_middle::dep_graph::DepNodeIndex;
use rustc_middle::ty::{self, TyCtxt};
use rustc_proc_macro as pm;
use rustc_span::{ErrorGuaranteed, LocalExpnId};

type ProcMacroClient = pm::bridge::client::Client;

pub(crate) fn expand_derive_macro_cached(
    invoc_id: LocalExpnId,
    input: TokenStream,
    ecx: &mut ExtCtxt<'_>,
    client: ProcMacroClient,
) -> Result<TokenStream, ()> {
    ty::tls::with(|tcx| {
        let input = &*tcx.arena.alloc(input);
        let key: (LocalExpnId, &TokenStream) = (invoc_id, input);

        QueryProcMacroExpandCtx::enter(ecx, client, move || tcx.derive_macro_expansion(key).cloned())
    })
}

pub(super) fn expand_attribute_macro_cached(
    annotation: TokenStream,
    annotated: TokenStream,
    ecx: &mut ExtCtxt<'_>,
    client: ProcMacroClient,
) -> Result<TokenStream, ErrorGuaranteed> {
    ty::tls::with(|tcx| {
        let annotation = &*tcx.arena.alloc(annotation);
        let annotated = &*tcx.arena.alloc(annotated);
        let key = (ecx.current_expansion.id, annotation, annotated);
        QueryProcMacroExpandCtx::enter(ecx, client, || tcx.attribute_macro_expansion(key).cloned())
    })
}

/// Cache attribute output tokens while keeping the annotation and item as distinct inputs.
pub(super) fn attribute_macro_expansion<'tcx>(
    tcx: TyCtxt<'tcx>,
    key: (LocalExpnId, &'tcx TokenStream, &'tcx TokenStream),
) -> Result<&'tcx TokenStream, ErrorGuaranteed> {
    let (invoc_id, annotation, annotated) = key;
    let expn_data = invoc_id.expn_data();
    let _ = tcx.crate_hash(expn_data.macro_def_id.unwrap().krate);

    QueryProcMacroExpandCtx::with(|ecx, client| {
        expand_attribute_macro(
            expn_data.call_site,
            annotation.clone(),
            annotated.clone(),
            ecx,
            client,
        )
        .map(|tokens| &*tcx.arena.alloc(tokens))
    })
}

/// Provide a query for computing the output of a derive macro.
pub(super) fn derive_macro_expansion<'tcx>(
    tcx: TyCtxt<'tcx>,
    key: (LocalExpnId, &'tcx TokenStream),
) -> Result<&'tcx TokenStream, ()> {
    let (invoc_id, input) = key;

    // Make sure that we invalidate the query when the crate defining the proc macro changes
    let _ = tcx.crate_hash(invoc_id.expn_data().macro_def_id.unwrap().krate);

    QueryProcMacroExpandCtx::with(|ecx, client| {
        expand_derive_macro(invoc_id, input.clone(), ecx, client).map(|ts| &*tcx.arena.alloc(ts))
    })
}

/// Prevent reuse when a queried macro observes state that its token inputs do not capture.
pub(super) fn invalidate_cached_expansion() {
    if PROC_MACRO_EXPAND_CTX.is_set() {
        ty::tls::with(|tcx| tcx.dep_graph.read_index(DepNodeIndex::FOREVER_RED_NODE));
    }
}

/// Stores the context necessary to expand a procedural macro via a query.
struct QueryProcMacroExpandCtx {
    /// Type-erased version of `&mut ExtCtxt`
    expansion_ctx: *mut (),
    client: ProcMacroClient,
}

impl QueryProcMacroExpandCtx {
    /// Store the extension context and the client into the thread local value.
    /// It will be accessible via the `with` method while `f` is active.
    fn enter<F, R>(ecx: &mut ExtCtxt<'_>, client: ProcMacroClient, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // We need erasure to get rid of the lifetime
        let ctx = Self { expansion_ctx: ecx as *mut _ as *mut (), client };
        PROC_MACRO_EXPAND_CTX.set(&ctx, f)
    }

    /// Accesses the thread local value of the procedural macro expansion context.
    /// Must be called while the `enter` function is active.
    fn with<F, R>(f: F) -> R
    where
        F: for<'a, 'b> FnOnce(&'b mut ExtCtxt<'a>, ProcMacroClient) -> R,
    {
        PROC_MACRO_EXPAND_CTX.with(|ctx| {
            let ectx = {
                let casted = ctx.expansion_ctx.cast::<ExtCtxt<'_>>();
                // SAFETY: We can only get the value from `with` while the `enter` function
                // is active (on the callstack), and that function's signature ensures that the
                // lifetime is valid.
                // If `with` is called at some other time, it will panic due to usage of
                // `scoped_tls::with`.
                unsafe { casted.as_mut().unwrap() }
            };

            f(ectx, ctx.client)
        })
    }
}

// Expansion queries borrow the current expansion context and client through scoped TLS.
scoped_tls::scoped_thread_local!(static PROC_MACRO_EXPAND_CTX: QueryProcMacroExpandCtx);
