use rustc_infer::traits::Obligation;
use rustc_middle::traits::ObligationCause;
use rustc_middle::traits::query::NoSolution;
pub use rustc_middle::traits::query::type_op::ProvePredicate;
use rustc_middle::ty::fast_reject::SimplifiedType;
use rustc_middle::ty::{self, ParamEnvAnd, TyCtxt, TypeVisitableExt};
use rustc_span::Span;

use crate::infer::canonical::{CanonicalQueryInput, CanonicalQueryResponse};
use crate::traits::{ObligationCtxt, sizedness_fast_path};

impl<'tcx> super::QueryTypeOp<'tcx> for ProvePredicate<'tcx> {
    type QueryResponse = ();

    fn try_fast_path(
        tcx: TyCtxt<'tcx>,
        key: &ParamEnvAnd<'tcx, Self>,
    ) -> Option<Self::QueryResponse> {
        if sizedness_fast_path(tcx, key.value.predicate, key.param_env) {
            return Some(());
        }

        if !cfg!(debug_assertions)
            && key.value.predicate.kind().bound_vars().is_empty()
            && let ty::PredicateKind::Clause(ty::ClauseKind::Trait(trait_pred)) =
                key.value.predicate.kind().skip_binder()
            && trait_pred.polarity == ty::PredicatePolarity::Positive
            && Some(trait_pred.def_id()) == tcx.lang_items().copy_trait()
        {
            let self_ty = trait_pred.self_ty();
            if !self_ty.has_non_region_infer()
                && !self_ty.has_aliases()
                && !self_ty.has_escaping_bound_vars()
                && !self_ty.references_error()
            {
                let kind = match *self_ty.kind() {
                    ty::Ref(_, _, ty::Mutability::Not) => {
                        Some(SimplifiedType::Ref(ty::Mutability::Not))
                    }
                    ty::RawPtr(_, mutability) => Some(SimplifiedType::Ptr(mutability)),
                    _ => None,
                };
                if kind.is_some_and(|kind| tcx.unconditional_copy_impl(kind))
                    // Both solvers prefer applicable caller bounds over impls. Preserve the
                    // region constraints and ambiguity checks produced by those bounds.
                    && !key.param_env.caller_bounds().iter().any(|clause| {
                        matches!(
                            clause.kind().skip_binder(),
                            ty::ClauseKind::Trait(bound)
                                if bound.polarity == ty::PredicatePolarity::Positive
                                    && bound.def_id() == trait_pred.def_id()
                        )
                    })
                {
                    return Some(());
                }
            }
        }

        if let ty::PredicateKind::Clause(ty::ClauseKind::WellFormed(term)) =
            key.value.predicate.kind().skip_binder()
            && term.is_trivially_wf(tcx)
        {
            return Some(());
        }

        None
    }

    fn perform_query(
        tcx: TyCtxt<'tcx>,
        canonicalized: CanonicalQueryInput<'tcx, ParamEnvAnd<'tcx, Self>>,
    ) -> Result<CanonicalQueryResponse<'tcx, ()>, NoSolution> {
        tcx.type_op_prove_predicate(canonicalized)
    }

    fn perform_locally_with_next_solver(
        ocx: &ObligationCtxt<'_, 'tcx>,
        key: ParamEnvAnd<'tcx, Self>,
        span: Span,
    ) -> Result<Self::QueryResponse, NoSolution> {
        ocx.register_obligation(Obligation::new(
            ocx.infcx.tcx,
            ObligationCause::dummy_with_span(span),
            key.param_env,
            key.value.predicate,
        ));
        Ok(())
    }
}
