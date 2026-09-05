//! Queries for checking whether a type implements one of a few common traits.

use rustc_hir::{def_id::DefId, lang_items::LangItem};
use rustc_infer::infer::TyCtxtInferExt;
use rustc_middle::query::Providers;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::DUMMY_SP;
use rustc_trait_selection::traits;

/// Summarizes ADT declarations whose WF obligations are only direct sizedness bounds.
///
/// The 32-argument limit keeps the requirements in two bits per argument without allocating.
/// A set high bit (`Sized`) subsumes the low bit (`MetaSized`). Positive `PointeeSized` bounds
/// are removed during lowering, so parameters with only that bound require no bits.
fn adt_wf_requirements(tcx: TyCtxt<'_>, def_id: DefId) -> Option<(u8, u64)> {
    let generics = tcx.generics_of(def_id);
    if generics.parent.is_some()
        || generics.parent_count != 0
        || generics.has_self
        || generics.own_params.len() > 32
        || generics.own_params.iter().enumerate().any(|(index, param)| {
            param.index as usize != index
                || !matches!(
                    param.kind,
                    ty::GenericParamDefKind::Lifetime | ty::GenericParamDefKind::Type { .. }
                )
        })
    {
        return None;
    }
    let predicates = tcx.predicates_of(def_id);
    if predicates.parent.is_some() {
        return None;
    }

    let mut requirements = 0;
    for &(clause, _) in predicates.predicates {
        if !clause.kind().bound_vars().is_empty() {
            return None;
        }
        let ty::ClauseKind::Trait(predicate) = clause.kind().skip_binder() else { return None };
        if predicate.polarity != ty::PredicatePolarity::Positive
            || predicate.trait_ref.args.len() != 1
        {
            return None;
        }
        let ty::Param(param) = *predicate.self_ty().kind() else { return None };
        let index = param.index as usize;
        if !matches!(generics.own_params.get(index)?.kind, ty::GenericParamDefKind::Type { .. }) {
            return None;
        }
        let bits = if Some(predicate.def_id()) == tcx.lang_items().sized_trait() {
            0b10
        } else if Some(predicate.def_id()) == tcx.lang_items().meta_sized_trait() {
            0b01
        } else {
            return None;
        };
        requirements |= bits << (index * 2);
    }
    Some((generics.own_params.len() as u8, requirements))
}

fn is_copy_raw<'tcx>(tcx: TyCtxt<'tcx>, query: ty::PseudoCanonicalInput<'tcx, Ty<'tcx>>) -> bool {
    is_trait_raw(tcx, query, LangItem::Copy)
}

fn is_use_cloned_raw<'tcx>(
    tcx: TyCtxt<'tcx>,
    query: ty::PseudoCanonicalInput<'tcx, Ty<'tcx>>,
) -> bool {
    is_trait_raw(tcx, query, LangItem::UseCloned)
}

fn is_sized_raw<'tcx>(tcx: TyCtxt<'tcx>, query: ty::PseudoCanonicalInput<'tcx, Ty<'tcx>>) -> bool {
    is_trait_raw(tcx, query, LangItem::Sized)
}

fn is_freeze_raw<'tcx>(tcx: TyCtxt<'tcx>, query: ty::PseudoCanonicalInput<'tcx, Ty<'tcx>>) -> bool {
    is_trait_raw(tcx, query, LangItem::Freeze)
}

fn is_unsafe_unpin_raw<'tcx>(
    tcx: TyCtxt<'tcx>,
    query: ty::PseudoCanonicalInput<'tcx, Ty<'tcx>>,
) -> bool {
    is_trait_raw(tcx, query, LangItem::UnsafeUnpin)
}

fn is_unpin_raw<'tcx>(tcx: TyCtxt<'tcx>, query: ty::PseudoCanonicalInput<'tcx, Ty<'tcx>>) -> bool {
    is_trait_raw(tcx, query, LangItem::Unpin)
}

fn is_async_drop_raw<'tcx>(
    tcx: TyCtxt<'tcx>,
    query: ty::PseudoCanonicalInput<'tcx, Ty<'tcx>>,
) -> bool {
    is_trait_raw(tcx, query, LangItem::AsyncDrop)
}

fn is_trait_raw<'tcx>(
    tcx: TyCtxt<'tcx>,
    query: ty::PseudoCanonicalInput<'tcx, Ty<'tcx>>,
    item: LangItem,
) -> bool {
    let (infcx, param_env) = tcx.infer_ctxt().build_with_typing_env(query.typing_env);
    let trait_def_id = tcx.require_lang_item(item, DUMMY_SP);
    traits::type_known_to_meet_bound_modulo_regions(&infcx, param_env, query.value, trait_def_id)
}

pub(crate) fn provide(providers: &mut Providers) {
    *providers = Providers {
        adt_wf_requirements,
        is_copy_raw,
        is_use_cloned_raw,
        is_sized_raw,
        is_freeze_raw,
        is_unsafe_unpin_raw,
        is_unpin_raw,
        is_async_drop_raw,
        ..*providers
    };
}
