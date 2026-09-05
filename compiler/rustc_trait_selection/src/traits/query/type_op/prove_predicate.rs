use rustc_infer::traits::Obligation;
use rustc_middle::traits::query::NoSolution;
pub use rustc_middle::traits::query::type_op::ProvePredicate;
use rustc_middle::traits::{ObligationCause, ObligationCauseCode};
use rustc_middle::ty::{self, ParamEnvAnd, SizedTraitKind, Ty, TyCtxt, TypeVisitableExt};
use rustc_span::Span;
use smallvec::SmallVec;

use crate::infer::InferCtxt;
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
            && term.is_trivially_wf(tcx)
        {
            return Some(());
        }

        None
    }

    fn try_perform_locally(
        infcx: &InferCtxt<'tcx>,
        key: &ParamEnvAnd<'tcx, Self>,
    ) -> Option<Self::QueryResponse> {
        let predicate = key.value.predicate.kind();
        if cfg!(debug_assertions)
            || !infcx.considering_regions
            || infcx.tcx.assumptions_on_binders()
            || !predicate.bound_vars().is_empty()
            || key.param_env.references_error()
        {
            return None;
        }
        let ty::PredicateKind::Clause(clause) = predicate.skip_binder() else { return None };
        if !matches!(
            clause,
            ty::ClauseKind::RegionOutlives(_)
                | ty::ClauseKind::TypeOutlives(_)
                | ty::ClauseKind::WellFormed(_)
        ) {
            return None;
        }
        let flags = ty::TypeFlags::HAS_ALIAS
            | ty::TypeFlags::HAS_ERROR
            | ty::TypeFlags::HAS_PLACEHOLDER
            | ty::TypeFlags::HAS_BOUND_VARS
            | ty::TypeFlags::HAS_RE_ERASED
            | ty::TypeFlags::HAS_TY_FRESH
            | ty::TypeFlags::HAS_CT_FRESH
            | (ty::TypeFlags::HAS_INFER - ty::TypeFlags::HAS_RE_INFER);
        if key.value.predicate.has_type_flags(flags) {
            return None;
        }

        // Prove structural WF requirements before registering their outlives constraints.
        // Keep all registration in this transaction so the normal TypeOp collector drains it.
        let cause = ObligationCause::dummy();
        match clause {
            ty::ClauseKind::RegionOutlives(predicate) => infcx.register_region_outlives_constraint(
                predicate,
                ty::VisibleForLeakCheck::Yes,
                &cause,
            ),
            ty::ClauseKind::TypeOutlives(ty::OutlivesPredicate(ty, region)) => {
                infcx.register_type_outlives_constraint(ty, region, &cause);
            }
            ty::ClauseKind::WellFormed(term) => {
                let ty = term.as_type()?;
                let mut references = SmallVec::new();
                if !wf_fast_path(infcx.tcx, ty, &mut 64, &mut references) {
                    return None;
                }
                for reference in references {
                    let ty::Ref(region, referent, _) = *reference.kind() else { unreachable!() };
                    let mut cause = ObligationCause::dummy();
                    cause.map_code(|_| ObligationCauseCode::ReferenceOutlivesReferent(reference));
                    infcx.register_type_outlives_constraint(referent, region, &cause);
                }
            }
            _ => unreachable!(),
        }
        Some(())
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

/// Proves structural well-formedness and collects each reference's outlives requirement.
/// Follows the reference order used by `WfPredicates`, without checking ADT fields. Callers
/// must register the collected constraints only after the entire bounded proof succeeds.
fn wf_fast_path<'tcx>(
    tcx: TyCtxt<'tcx>,
    ty: Ty<'tcx>,
    budget: &mut u8,
    references: &mut SmallVec<[Ty<'tcx>; 4]>,
) -> bool {
    let Some(remaining) = budget.checked_sub(1) else { return false };
    *budget = remaining;
    if ty.is_trivially_wf(tcx) {
        return true;
    }
    let (def, args) = match *ty.kind() {
        ty::Ref(_, referent, _) => {
            references.push(ty);
            return wf_fast_path(tcx, referent, budget, references);
        }
        ty::RawPtr(pointee, _) => return wf_fast_path(tcx, pointee, budget, references),
        ty::Adt(def, args) => (def, args),
        _ => return false,
    };
    let Some((arity, requirements)) = tcx.adt_wf_requirements(def.did()) else { return false };
    if args.len() != usize::from(arity) {
        return false;
    }
    for (index, arg) in args.iter().enumerate() {
        match arg.kind() {
            ty::GenericArgKind::Lifetime(_) => {
                let Some(remaining) = budget.checked_sub(1) else { return false };
                *budget = remaining;
            }
            ty::GenericArgKind::Type(ty) => {
                let bits = (requirements >> (index * 2)) & 0b11;
                let sizedness = match bits {
                    0 => None,
                    0b01 => Some(SizedTraitKind::MetaSized),
                    _ => Some(SizedTraitKind::Sized),
                };
                if sizedness.is_some_and(|sizedness| !ty.has_trivial_sizedness(tcx, sizedness))
                    || !wf_fast_path(tcx, ty, budget, references)
                {
                    return false;
                }
            }
            ty::GenericArgKind::Const(_) => return false,
        }
    }
    true
}
