use rustc_errors::ErrorGuaranteed;
use rustc_hir::LangItem;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_infer::traits::Obligation;
use rustc_macros::TypeVisitable;
use rustc_middle::traits::ObligationCause;
pub use rustc_middle::traits::query::type_op::ProvePredicate;
use rustc_middle::traits::query::{CanonicalTypeOpProvePredicateGoal, NoSolution};
use rustc_middle::ty::fast_reject::SimplifiedType;
use rustc_middle::ty::{self, ParamEnvAnd, Ty, TyCtxt, TypeVisitableExt, Upcast};
use rustc_span::Span;

use super::{TypeOp, TypeOpOutput, collect_fast_path_output};
use crate::infer::InferCtxt;
use crate::infer::canonical::{CanonicalQueryInput, CanonicalQueryResponse};
use crate::traits::{ObligationCtxt, sizedness_fast_path};

/// Proves an unbound, positive `Copy` predicate without constructing it when the impl is
/// unconditional. Other cases use the ordinary predicate operation, including its diagnostics.
#[derive(Debug, TypeVisitable)]
pub struct ProveCopy<'tcx> {
    self_ty: Ty<'tcx>,
    copy_trait: DefId,
}

impl<'tcx> ProveCopy<'tcx> {
    #[inline]
    pub fn new(tcx: TyCtxt<'tcx>, self_ty: Ty<'tcx>, span: Span) -> Self {
        Self { self_ty, copy_trait: tcx.require_lang_item(LangItem::Copy, span) }
    }
}

impl<'tcx> TypeOp<'tcx> for ParamEnvAnd<'tcx, ProveCopy<'tcx>> {
    type Output = ();
    type ErrorInfo = CanonicalTypeOpProvePredicateGoal<'tcx>;

    fn fully_perform(
        self,
        infcx: &InferCtxt<'tcx>,
        root_def_id: LocalDefId,
        span: Span,
    ) -> Result<TypeOpOutput<'tcx, Self>, ErrorGuaranteed> {
        let tcx = infcx.tcx;
        if !infcx.disable_trait_solver_fast_paths()
            && copy_fast_path(tcx, self.param_env, self.value.self_ty, self.value.copy_trait)
        {
            return collect_fast_path_output(infcx, ());
        }

        let predicate =
            ty::TraitRef::new(tcx, self.value.copy_trait, [self.value.self_ty]).upcast(tcx);
        let TypeOpOutput { output, constraints, error_info } = self
            .param_env
            .and(ProvePredicate { predicate })
            .fully_perform(infcx, root_def_id, span)?;
        Ok(TypeOpOutput { output, constraints, error_info })
    }
}

fn copy_fast_path<'tcx>(
    tcx: TyCtxt<'tcx>,
    param_env: ty::ParamEnv<'tcx>,
    self_ty: Ty<'tcx>,
    copy_trait: DefId,
) -> bool {
    if cfg!(debug_assertions)
        || self_ty.has_non_region_infer()
        || self_ty.has_aliases()
        || self_ty.has_escaping_bound_vars()
        || self_ty.references_error()
    {
        return false;
    }
    let kind = match *self_ty.kind() {
        ty::Ref(_, _, ty::Mutability::Not) => SimplifiedType::Ref(ty::Mutability::Not),
        ty::RawPtr(_, mutability) => SimplifiedType::Ptr(mutability),
        _ => return false,
    };
    tcx.unconditional_copy_impl(kind)
        // Both solvers prefer applicable caller bounds over impls. Preserve the region
        // constraints and ambiguity checks produced by those bounds.
        && !param_env.caller_bounds().iter().any(|clause| {
            matches!(
                clause.kind().skip_binder(),
                ty::ClauseKind::Trait(bound)
                    if bound.polarity == ty::PredicatePolarity::Positive
                        && bound.def_id() == copy_trait
            )
        })
}

impl<'tcx> super::QueryTypeOp<'tcx> for ProvePredicate<'tcx> {
    type QueryResponse = ();

    fn try_fast_path(
        tcx: TyCtxt<'tcx>,
        key: &ParamEnvAnd<'tcx, Self>,
    ) -> Option<Self::QueryResponse> {
        if sizedness_fast_path(tcx, key.value.predicate, key.param_env) {
            return Some(());
        }

        if key.value.predicate.kind().bound_vars().is_empty()
            && let ty::PredicateKind::Clause(ty::ClauseKind::Trait(trait_pred)) =
                key.value.predicate.kind().skip_binder()
            && trait_pred.polarity == ty::PredicatePolarity::Positive
            && Some(trait_pred.def_id()) == tcx.lang_items().copy_trait()
            && copy_fast_path(tcx, key.param_env, trait_pred.self_ty(), trait_pred.def_id())
        {
            return Some(());
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
