//! Queries for checking whether a type implements one of a few common traits.

use rustc_hir::{def_id::DefId, lang_items::LangItem};
use rustc_infer::infer::TyCtxtInferExt;
use rustc_middle::query::Providers;
use rustc_middle::ty::fast_reject::SimplifiedType;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::DUMMY_SP;
use rustc_trait_selection::traits;

/// Recognizes an unconditional impl for a pointer or an ADT with only lifetime parameters.
///
/// Matching every impl parameter to a distinct self-type argument ensures that confirmation
/// cannot constrain the goal. Impl validity is checked separately, so local ADT impls do not
/// require inspecting their fields or proving their well-formedness here.
fn unconditional_trait_impl(tcx: TyCtxt<'_>, key: (DefId, SimplifiedType)) -> bool {
    let (trait_id, kind) = key;
    if !matches!(
        kind,
        SimplifiedType::Ref(ty::Mutability::Not) | SimplifiedType::Ptr(_) | SimplifiedType::Adt(_)
    ) {
        return false;
    }
    let is_copy = Some(trait_id) == tcx.lang_items().copy_trait();
    // Pointer shapes are specific to Copy. Other compiler-provided traits can have special
    // selection rules, so only consider ordinary traits on ADTs here.
    if !is_copy
        && (!matches!(kind, SimplifiedType::Adt(_))
            || tcx.lang_items().from_def_id(trait_id).is_some()
            || tcx.trait_is_auto(trait_id)
            || tcx.is_trait_alias(trait_id))
    {
        return false;
    }
    let impls = tcx.trait_impls_of(trait_id);
    if !impls.blanket_impls().is_empty() {
        return false;
    }
    let Some([impl_id]) = impls.non_blanket_impls().get(&kind).map(|impls| impls.as_slice()) else {
        return false;
    };
    let impl_id = *impl_id;
    // Preserve the imported-only restriction for pointer impls in custom core libraries.
    if (impl_id.is_local() && !matches!(kind, SimplifiedType::Adt(_)))
        || tcx.impl_polarity(impl_id) != ty::ImplPolarity::Positive
        || !tcx.defaultness(impl_id).is_final()
    {
        return false;
    }
    let generics = tcx.generics_of(impl_id);
    if generics.parent.is_some() || generics.parent_count != 0 || generics.has_self {
        return false;
    }
    let self_ty = tcx.type_of(impl_id).instantiate_identity().skip_norm_wip();
    let shape_matches = match (kind, *self_ty.kind()) {
        (
            SimplifiedType::Ref(ty::Mutability::Not),
            ty::Ref(region, pointee, ty::Mutability::Not),
        ) => {
            matches!(
                generics.own_params.as_slice(),
                [
                    ty::GenericParamDef { index: 0, kind: ty::GenericParamDefKind::Lifetime, .. },
                    ty::GenericParamDef {
                        index: 1,
                        kind: ty::GenericParamDefKind::Type { .. },
                        ..
                    },
                ]
            ) && matches!(region.kind(), ty::ReEarlyParam(region) if region.index == 0)
                && matches!(pointee.kind(), ty::Param(param) if param.index == 1)
        }
        (SimplifiedType::Ptr(expected_mutability), ty::RawPtr(pointee, mutability)) => {
            expected_mutability == mutability
                && matches!(
                    generics.own_params.as_slice(),
                    [ty::GenericParamDef {
                        index: 0,
                        kind: ty::GenericParamDefKind::Type { .. },
                        ..
                    },]
                )
                && matches!(pointee.kind(), ty::Param(param) if param.index == 0)
        }
        (SimplifiedType::Adt(expected_def_id), ty::Adt(def, args)) => {
            def.did() == expected_def_id
                && generics.own_params.iter().enumerate().all(|(index, param)| {
                    param.index as usize == index
                        && matches!(param.kind, ty::GenericParamDefKind::Lifetime)
                })
                && args == ty::GenericArgs::identity_for_item(tcx, impl_id)
        }
        _ => false,
    };
    if !shape_matches {
        return false;
    }
    let predicates = tcx.predicates_of(impl_id);
    predicates.parent.is_none()
        && predicates.predicates.is_empty()
        && tcx.impl_super_outlives(impl_id).skip_binder().is_empty()
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
        is_copy_raw,
        unconditional_trait_impl,
        is_use_cloned_raw,
        is_sized_raw,
        is_freeze_raw,
        is_unsafe_unpin_raw,
        is_unpin_raw,
        is_async_drop_raw,
        ..*providers
    };
}
