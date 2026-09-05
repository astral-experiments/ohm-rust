use rustc_infer::traits::Obligation;
use rustc_middle::traits::ObligationCause;
use rustc_middle::traits::query::NoSolution;
pub use rustc_middle::traits::query::type_op::ProvePredicate;
use rustc_middle::ty::{self, ParamEnvAnd, SizedTraitKind, Ty, TyCtxt, TypeVisitableExt};
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

        if let ty::PredicateKind::Clause(ty::ClauseKind::WellFormed(term)) =
            key.value.predicate.kind().skip_binder()
        {
            if term.is_trivially_wf(tcx) {
                return Some(());
            }

            if !cfg!(debug_assertions)
                && key.value.predicate.kind().bound_vars().is_empty()
                && let Some(ty) = term.as_type()
                && !ty.has_non_region_infer()
                && !ty.has_aliases()
                && !ty.has_escaping_bound_vars()
                && !ty.references_error()
                && matches!(ty.kind(), ty::Adt(..))
                && nominal_wf_fast_path(tcx, ty, &mut 64)
            {
                return Some(());
            }
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

/// Checks a bounded number of nominal types and arguments without creating obligations.
fn nominal_wf_fast_path<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>, budget: &mut u8) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    if ty.is_trivially_wf(tcx) {
        return true;
    }
    let ty::Adt(def, args) = *ty.kind() else { return false };
    let Some((arity, requirements)) = tcx.adt_wf_requirements(def.did()) else { return false };
    if args.len() != usize::from(arity) {
        return false;
    }
    for (index, arg) in args.iter().enumerate() {
        match arg.kind() {
            ty::GenericArgKind::Lifetime(_) => {
                if *budget == 0 {
                    return false;
                }
                *budget -= 1;
            }
            ty::GenericArgKind::Type(ty) => {
                let bits = (requirements >> (index * 2)) & 0b11;
                let sizedness = match bits {
                    0 => None,
                    0b01 => Some(SizedTraitKind::MetaSized),
                    _ => Some(SizedTraitKind::Sized),
                };
                if sizedness.is_some_and(|sizedness| !ty.has_trivial_sizedness(tcx, sizedness))
                    || !nominal_wf_fast_path(tcx, ty, budget)
                {
                    return false;
                }
            }
            ty::GenericArgKind::Const(_) => return false,
        }
    }
    true
}
