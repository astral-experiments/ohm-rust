//! Metadata-only consumers need const MIR and hidden opaque types, but not
//! ordinary function MIR. Prepare these bodies and synthetic coroutine
//! definitions before freezing the definition table and encoding metadata.

use rustc_data_structures::fx::FxHashSet;
use rustc_data_structures::sync::par_for_each_in;
use rustc_hir as hir;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::{LocalDefId, LocalDefIdSet};
use rustc_middle::ty::{self, TyCtxt};
use rustc_session::config::{CrateType, OutputType};

use super::{should_encode_const, should_encode_mir};

pub(super) fn enabled(tcx: TyCtxt<'_>) -> bool {
    tcx.sess.opts.unstable_opts.ohm_early_metadata
        && !tcx.sess.opts.output_types.should_codegen()
        && !tcx.sess.opts.unstable_opts.always_encode_mir
        && !tcx.sess.opts.unstable_opts.validate_mir
        && !tcx.is_sdylib_interface_build()
        && tcx.sess.opts.output_types.contains_key(&OutputType::Metadata)
        && tcx.crate_types() == [CrateType::Rlib]
}

pub(super) fn enclosing_body(tcx: TyCtxt<'_>, def_id: LocalDefId) -> Option<LocalDefId> {
    match tcx.def_kind(def_id) {
        DefKind::Closure | DefKind::SyntheticCoroutineBody => {
            return Some(tcx.typeck_root_def_id_local(def_id));
        }
        DefKind::AnonConst if tcx.is_typeck_child(def_id.to_def_id()) => {
            return Some(tcx.typeck_root_def_id_local(def_id));
        }
        DefKind::AnonConst => {}
        _ => return None,
    }
    for (id, node) in tcx.hir_parent_iter(tcx.local_def_id_to_hir_id(def_id)) {
        match node {
            hir::Node::Expr(_)
            | hir::Node::Block(_)
            | hir::Node::Stmt(_)
            | hir::Node::LetStmt(_)
            | hir::Node::Pat(_)
            | hir::Node::PatExpr(_) => {
                return Some(tcx.typeck_root_def_id_local(tcx.hir_enclosing_body_owner(id)));
            }
            hir::Node::Item(_)
            | hir::Node::TraitItem(_)
            | hir::Node::ImplItem(_)
            | hir::Node::ForeignItem(_) => return None,
            _ => {}
        }
    }
    None
}

pub(super) fn interface_bodies(tcx: TyCtxt<'_>) -> FxHashSet<LocalDefId> {
    let mut bodies = FxHashSet::default();
    for &def_id in tcx.mir_keys(()) {
        if enclosing_body(tcx, def_id).is_some() {
            continue;
        }
        if should_encode_mir(tcx, &LocalDefIdSet::default(), def_id).0 {
            bodies.insert(tcx.typeck_root_def_id_local(def_id));
        }
    }
    let mut type_alias_opaques = FxHashSet::default();
    let items = tcx.hir_crate_items(());
    for def_id in items.definitions().chain(items.opaques()) {
        match tcx.def_kind(def_id) {
            DefKind::Static { .. } => {
                bodies.insert(def_id);
            }
            DefKind::OpaqueTy => match tcx.local_opaque_ty_origin(def_id) {
                hir::OpaqueTyOrigin::FnReturn { parent, .. }
                | hir::OpaqueTyOrigin::AsyncFn { parent, .. } => {
                    bodies.insert(tcx.typeck_root_def_id_local(parent));
                }
                hir::OpaqueTyOrigin::TyAlias { .. } => {
                    type_alias_opaques.insert(def_id);
                }
            },
            _ => {}
        }
    }
    if !type_alias_opaques.is_empty() {
        for owner in tcx.hir_body_owners() {
            if tcx
                .opaque_types_defined_by(owner)
                .iter()
                .any(|opaque| type_alias_opaques.contains(&opaque))
            {
                bodies.insert(tcx.typeck_root_def_id_local(owner));
            }
        }
    }
    bodies
}

pub(crate) fn prepare(tcx: TyCtxt<'_>) -> bool {
    if !enabled(tcx) {
        return false;
    }
    tcx.ensure_ok().analysis_interfaces(());
    if tcx.dcx().has_errors_or_delayed_bugs().is_some() {
        return false;
    }
    tcx.par_hir_body_owners(|def_id| {
        if let DefKind::Static { .. } = tcx.def_kind(def_id) {
            tcx.ensure_ok().eval_static_initializer(def_id);
        }
    });
    for def_id in tcx.hir_body_owners() {
        if let Some(hir::CoroutineKind::Desugared(_, hir::CoroutineSource::Closure)) =
            tcx.coroutine_kind(def_id.to_def_id())
        {
            let item = tcx.hir_get_parent_item(tcx.local_def_id_to_hir_id(def_id)).def_id;
            if tcx.hir_maybe_body_owned_by(item).is_some() {
                tcx.ensure_ok().typeck(item);
            }
            if tcx.needs_coroutine_by_move_body_def_id(def_id.to_def_id()) {
                tcx.ensure_done().coroutine_by_move_body_def_id(def_id);
            }
        }
    }
    let bodies = interface_bodies(tcx);
    #[allow(rustc::potential_query_instability)]
    let mut sorted: Vec<_> = bodies.iter().copied().collect();
    sorted.sort_by_key(|def_id| def_id.local_def_index);
    par_for_each_in(&sorted, |&&def_id| {
        if tcx.hir_maybe_body_owned_by(def_id).is_some()
            && !(tcx.def_kind(def_id) == DefKind::AnonConst
                && tcx.anon_const_kind(def_id) != ty::AnonConstKind::NonTypeSystemInline)
        {
            tcx.ensure_ok().typeck(def_id);
        }
    });
    par_for_each_in(tcx.mir_keys(()), |&&def_id| {
        if enclosing_body(tcx, def_id).is_some_and(|body| !bodies.contains(&body)) {
            return;
        }
        let encode_const = should_encode_mir(tcx, &LocalDefIdSet::default(), def_id).0;
        let coroutine =
            matches!(tcx.def_kind(def_id), DefKind::Closure | DefKind::SyntheticCoroutineBody)
                && tcx.is_coroutine(def_id.to_def_id());
        if encode_const {
            if tcx.trivial_const(def_id).is_none() {
                tcx.ensure_done().mir_for_ctfe(def_id);
            }
            tcx.ensure_ok().thir_abstract_const(def_id);
            if should_encode_const(tcx.def_kind(def_id)) {
                tcx.ensure_ok().mir_const_qualif(def_id);
            }
        }
        if (encode_const || coroutine) && !tcx.is_trivial_const(def_id) {
            tcx.ensure_done().promoted_mir(def_id);
        }
        if coroutine {
            tcx.ensure_ok().mir_coroutine_runtime_info(def_id);
            tcx.ensure_ok().mir_coroutine_witnesses(def_id);
        }
    });
    tcx.dcx().has_errors_or_delayed_bugs().is_none()
}
