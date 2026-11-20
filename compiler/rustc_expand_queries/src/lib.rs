#![allow(internal_features, reason = "proc macro internals")]
#![feature(proc_macro_internals)]

use rustc_expand::syntax_codec::{SyntaxCacheKey, SyntaxCacheOp};
use rustc_middle::ty::TyCtxt;

mod derive;

pub fn setup_callbacks() {
    use rustc_expand::syntax_codec::{PARENT_HASH, PARENT_ID};
    PARENT_ID.swap(
        &((|hash| {
            rustc_middle::ty::tls::with_opt(|tcx| tcx?.def_path_hash_to_def_id(hash)?.as_local())
        }) as _),
    );
    PARENT_HASH.swap(
        &((|id: rustc_span::def_id::LocalDefId| {
            rustc_middle::ty::tls::with_opt(|tcx| Some(tcx?.def_path_hash(id.to_def_id())))
        }) as _),
    );

    rustc_expand::syntax_codec::SYNTAX_CACHE.swap(&(syntax_cache as _));
    rustc_expand::proc_macro::EXPAND_ATTRIBUTE_MACRO_CACHED
        .swap(&(derive::expand_attribute_macro_cached as _));
    rustc_expand::proc_macro::INVALIDATE_CACHED_EXPANSION
        .swap(&(derive::invalidate_cached_expansion as _));
    rustc_expand::proc_macro::PROC_MACRO_SOURCE_CODE.swap(&(derive::proc_macro_source_code as _));
    rustc_expand::proc_macro::EXPAND_DERIVE_MACRO_CACHED
        .swap(&(derive::expand_derive_macro_cached as _));
}

pub fn provide(providers: &mut rustc_middle::query::Providers) {
    providers.ohm_macro_syntax = ohm_macro_syntax;
    providers.ohm_module_syntax = ohm_module_syntax;
    providers.attribute_macro_expansion = derive::attribute_macro_expansion;
    providers.derive_macro_expansion = derive::derive_macro_expansion;
}

// A miss is parsed or expanded in rustc_expand. Supply its completed result to the
// no_force query provider on this thread. A green query may already have a
// value for different inputs, so always clear the
// slot when the query returns.
thread_local! {
    static SYNTAX_BYTES: std::cell::RefCell<Option<Vec<u8>>> = const { std::cell::RefCell::new(None) };
}

fn syntax_cache(key: SyntaxCacheKey, op: SyntaxCacheOp<'_>) {
    use rustc_middle::dep_graph::{DepKind, DepNode};
    rustc_middle::ty::tls::with_opt(|tcx| {
        let Some(tcx) = tcx else { return };
        let query = || match key {
            SyntaxCacheKey::Module(id) => tcx.ohm_module_syntax(id),
            SyntaxCacheKey::Macro(id) => tcx.ohm_macro_syntax(id),
        };
        match op {
            SyntaxCacheOp::Load(decode) => {
                let node = match key {
                    SyntaxCacheKey::Module(id) => {
                        DepNode::construct(tcx, DepKind::ohm_module_syntax, &id)
                    }
                    SyntaxCacheKey::Macro(id) => {
                        DepNode::construct(tcx, DepKind::ohm_macro_syntax, &id)
                    }
                };
                if tcx.dep_graph.try_mark_green(tcx, &node).is_some() {
                    let bytes = query();
                    if !bytes.is_empty() {
                        decode(bytes)
                    }
                }
            }
            SyntaxCacheOp::Save(bytes) => {
                SYNTAX_BYTES.replace(Some(bytes));
                let _ = query();
                SYNTAX_BYTES.take();
            }
        }
    });
}

fn ohm_module_syntax(tcx: TyCtxt<'_>, id: rustc_span::StableSourceFileId) -> &[u8] {
    let _ = tcx.metadata_source_file_hash(id);
    SYNTAX_BYTES.take().map_or(&[], |bytes| tcx.arena.alloc_slice(&bytes))
}

fn ohm_macro_syntax(
    tcx: TyCtxt<'_>,
    (file, krate): (rustc_span::StableSourceFileId, rustc_span::def_id::CrateNum),
) -> &[u8] {
    let _ = tcx.features_query(());
    let _ = tcx.public_api_hash(krate);
    let _ = tcx.metadata_source_file_hash(file);
    SYNTAX_BYTES.take().map_or(&[], |bytes| tcx.arena.alloc_slice(&bytes))
}
