use rustc_infer::infer::InferOk;
use rustc_infer::infer::canonical::QueryRegionConstraint;
use rustc_infer::infer::resolve::OpportunisticRegionResolver;
use rustc_infer::traits::query::type_op::ImpliedOutlivesBounds;
use rustc_macros::extension;
use rustc_middle::infer::canonical::{OriginalQueryValues, QueryRegionConstraints};
pub use rustc_middle::traits::query::OutlivesBound;
use rustc_middle::ty::{self, ParamEnv, Ty, TypeFolder, TypeVisitableExt};
use rustc_span::def_id::LocalDefId;
use rustc_span::sym;
use tracing::instrument;

use crate::infer::InferCtxt;
use crate::traits::ObligationCause;

/// Implied bounds are region relationships that we deduce
/// automatically. The idea is that (e.g.) a caller must check that a
/// function's argument types are well-formed immediately before
/// calling that fn, and hence the *callee* can assume that its
/// argument types are well-formed. This may imply certain relationships
/// between generic parameters. For example:
/// ```
/// fn foo<T>(x: &T) {}
/// ```
/// can only be called with a `'a` and `T` such that `&'a T` is WF.
/// For `&'a T` to be WF, `T: 'a` must hold. So we can assume `T: 'a`.
///
/// # Parameters
///
/// - `param_env`, the where-clauses in scope
/// - `body_id`, the body-id to use when normalizing assoc types.
///   Note that this may cause outlives obligations to be injected
///   into the inference context with this body-id.
/// - `ty`, the type that we are supposed to assume is WF.
#[instrument(level = "debug", skip(infcx, param_env, body_id), ret)]
fn implied_outlives_bounds<'a, 'tcx>(
    infcx: &'a InferCtxt<'tcx>,
    param_env: ty::ParamEnv<'tcx>,
    body_id: LocalDefId,
    ty: Ty<'tcx>,
    disable_implied_bounds_hack: bool,
) -> Vec<OutlivesBound<'tcx>> {
    let ty = infcx.resolve_vars_if_possible(ty);
    let ty = OpportunisticRegionResolver::new(infcx).fold_ty(ty);

    // We do not expect existential variables in implied bounds.
    // We may however encounter unconstrained lifetime variables
    // in very rare cases.
    //
    // See `ui/implied-bounds/implied-bounds-unconstrained-2.rs` for
    // an example.
    assert!(!ty.has_non_region_infer());

    // A fieldless struct with only concrete `usize` const arguments and no
    // other bounds cannot imply any lifetime relationships. In particular,
    // there is nothing to normalize or relate to the caller's environment.
    let tcx = infcx.tcx;
    if param_env.caller_bounds().is_empty()
        && let ty::Adt(adt, args) = ty.kind()
        && adt.is_struct()
        && adt.non_enum_variant().fields.is_empty()
        && adt.did().is_local()
        && args.iter().all(|arg| {
            matches!(arg.kind(), ty::GenericArgKind::Const(ct)
                if matches!(ct.kind(), ty::ConstKind::Value(value) if value.ty == tcx.types.usize))
        })
        && let generics = tcx.generics_of(adt.did())
        && generics.parent_count == 0
        && generics.own_params.len() == args.len()
        && generics.own_params.iter().all(|p| {
            matches!(p.kind, ty::GenericParamDefKind::Const { has_default: false })
        })
        && let predicates = tcx.predicates_of(adt.did())
        && predicates.parent.is_none()
        && predicates.predicates.len() == args.len()
        && predicates.predicates.iter().all(|(clause, _)| {
            matches!(clause.kind().no_bound_vars(),
                Some(ty::ClauseKind::ConstArgHasType(ct, t))
                    if matches!(ct.kind(), ty::ConstKind::Param(_)) && t == tcx.types.usize)
        })
        // Preserve the compatibility hack for `bevy_ecs::ParamSet`.
        && !(tcx.item_name(adt.did()) == sym::ParamSet
            && tcx.crate_name(adt.did().krate) == sym::bevy_ecs)
    {
        return vec![];
    }

    let mut canonical_var_values = OriginalQueryValues::default();
    let input = ImpliedOutlivesBounds { ty };
    let canonical = infcx.canonicalize_query(param_env.and(input), &mut canonical_var_values);
    let implied_bounds_result =
        infcx.tcx.implied_outlives_bounds((canonical, disable_implied_bounds_hack));
    let Ok(canonical_result) = implied_bounds_result else {
        return vec![];
    };

    let mut constraints = QueryRegionConstraints::default();
    let span = infcx.tcx.def_span(body_id);
    let Ok(InferOk { value: mut bounds, obligations }) = infcx
        .instantiate_nll_query_response_and_region_obligations(
            &ObligationCause::dummy_with_span(span),
            param_env,
            &canonical_var_values,
            canonical_result,
            &mut constraints,
        )
    else {
        return vec![];
    };
    assert_eq!(obligations.len(), 0);

    // Because of #109628, we may have unexpected placeholders. Ignore them!
    // FIXME(#109628): panic in this case once the issue is fixed.
    bounds.retain(|bound| !bound.has_placeholders());

    if !constraints.is_empty() {
        // FIXME(higher_ranked_auto): Should we register assumptions here?
        // We otherwise would get spurious errors if normalizing an implied
        // outlives bound required proving some higher-ranked coroutine obl.
        let QueryRegionConstraints { constraints, assumptions: _ } = constraints;
        let cause = ObligationCause::misc(span, body_id);
        for &QueryRegionConstraint { constraint, visible_for_leak_check: vis, .. } in &constraints {
            match constraint {
                ty::RegionConstraint::Outlives(predicate) => {
                    infcx.register_outlives_constraint(predicate, vis, &cause)
                }
                ty::RegionConstraint::Eq(predicate) => {
                    infcx.register_region_eq_constraint(predicate, vis, &cause)
                }
            }
        }
    };

    bounds
}

#[extension(pub trait InferCtxtExt<'tcx>)]
impl<'tcx> InferCtxt<'tcx> {
    /// Do *NOT* call this directly. You probably want to construct a `OutlivesEnvironment`
    /// instead if you're interested in the implied bounds for a given signature.
    fn implied_bounds_tys<Tys: IntoIterator<Item = Ty<'tcx>>>(
        &self,
        body_id: LocalDefId,
        param_env: ParamEnv<'tcx>,
        tys: Tys,
        disable_implied_bounds_hack: bool,
    ) -> impl Iterator<Item = OutlivesBound<'tcx>> {
        tys.into_iter().flat_map(move |ty| {
            implied_outlives_bounds(self, param_env, body_id, ty, disable_implied_bounds_hack)
        })
    }
}
