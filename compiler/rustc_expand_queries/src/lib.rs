#![allow(internal_features, reason = "proc macro internals")]
#![feature(proc_macro_internals)]

use rustc_middle::ty::TyCtxt;

mod derive;

pub fn setup_callbacks() {
    rustc_expand::module::MODULE_CACHE.swap(&(syntax_cache as _));
    rustc_expand::proc_macro::EXPAND_ATTRIBUTE_MACRO_CACHED
        .swap(&(derive::expand_attribute_macro_cached as _));
    rustc_expand::proc_macro::INVALIDATE_CACHED_EXPANSION
        .swap(&(derive::invalidate_cached_expansion as _));
    rustc_expand::proc_macro::PROC_MACRO_SOURCE_CODE.swap(&(derive::proc_macro_source_code as _));
    rustc_expand::proc_macro::EXPAND_DERIVE_MACRO_CACHED
        .swap(&(derive::expand_derive_macro_cached as _));
}

pub fn provide(providers: &mut rustc_middle::query::Providers) {
    providers.ohm_module_syntax = ohm_module_syntax;
    providers.attribute_macro_expansion = derive::attribute_macro_expansion;
    providers.derive_macro_expansion = derive::derive_macro_expansion;
}

// A miss is parsed in rustc_expand. Supply its completed result to the
// no_force query provider on this thread. A green query may already have a
// value for a different declaration of the same file, so always clear the
// slot when the query returns.
thread_local! {
    static SYNTAX_BYTES: std::cell::RefCell<Option<Vec<u8>>> = const { std::cell::RefCell::new(None) };
}

fn syntax_cache(id: rustc_span::StableSourceFileId, op: rustc_expand::module::ModuleCacheOp<'_>) {
    use rustc_expand::module::ModuleCacheOp;
    use rustc_middle::dep_graph::{DepKind, DepNode};
    rustc_middle::ty::tls::with_opt(|tcx| {
        let Some(tcx) = tcx else { return };
        match op {
            ModuleCacheOp::Load(decode) => {
                let node = DepNode::construct(tcx, DepKind::ohm_module_syntax, &id);
                if tcx.dep_graph.try_mark_green(tcx, &node).is_some() {
                    let bytes = tcx.ohm_module_syntax(id);
                    if !bytes.is_empty() {
                        decode(bytes)
                    }
                }
            }
            ModuleCacheOp::Save(bytes) => {
                SYNTAX_BYTES.replace(Some(bytes));
                let _ = tcx.ohm_module_syntax(id);
                SYNTAX_BYTES.take();
            }
        }
    });
}

fn ohm_module_syntax(tcx: TyCtxt<'_>, id: rustc_span::StableSourceFileId) -> &[u8] {
    let _ = tcx.source_file_hash(id);
    SYNTAX_BYTES.take().map_or(&[], |bytes| tcx.arena.alloc_slice(&bytes))
}
