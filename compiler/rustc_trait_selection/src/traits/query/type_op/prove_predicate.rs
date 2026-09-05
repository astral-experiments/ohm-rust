mod local_wf_coverage;

use std::ops::ControlFlow;

use rustc_infer::traits::Obligation;
use rustc_middle::traits::ObligationCause;
use rustc_middle::traits::query::NoSolution;
pub use rustc_middle::traits::query::type_op::ProvePredicate;
use rustc_middle::ty::{
    self, ParamEnvAnd, Ty, TyCtxt, TypeSuperVisitable, TypeVisitable, TypeVisitableExt, TypeVisitor,
};
use rustc_span::Span;

use crate::infer::InferCtxt;
use crate::infer::canonical::{CanonicalQueryInput, CanonicalQueryResponse};
use crate::traits::{ObligationCtxt, sizedness_fast_path, wf};

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
        let mut coverage = local_wf_coverage::Coverage::start(infcx.tcx, key);
        let predicate = key.value.predicate.kind();
        if cfg!(debug_assertions) {
            coverage.reject("debug-assertions");
            return None;
        }
        if !infcx.considering_regions {
            coverage.reject("regions-disabled");
            return None;
        }
        if infcx.tcx.assumptions_on_binders() {
            coverage.reject("assumptions-on-binders");
            return None;
        }
        if !predicate.bound_vars().is_empty() {
            coverage.reject("root-binder");
            return None;
        }
        if key.param_env.references_error() {
            coverage.reject("param-env-error");
            return None;
        }
        let ty::PredicateKind::Clause(clause) = predicate.skip_binder() else {
            coverage.reject("unsupported-predicate-kind");
            return None;
        };
        if !matches!(
            clause,
            ty::ClauseKind::RegionOutlives(_)
                | ty::ClauseKind::TypeOutlives(_)
                | ty::ClauseKind::WellFormed(_)
        ) {
            coverage.reject("unsupported-clause-kind");
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
            coverage.reject_predicate(infcx.tcx, "root-flags", key.value.predicate, flags);
            return None;
        }

        // Prove all requirements before registering constraints in this transaction,
        // where the normal TypeOp collector drains them.
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
                let Some(ty) = term.as_type() else {
                    coverage.reject("non-type-wf-term");
                    return None;
                };
                if let ControlFlow::Break(failure) = ty.visit_with(&mut WfInput { remaining: 64 }) {
                    coverage.reject_input(infcx.tcx, failure);
                    return None;
                }
                let Some(obligations) = wf::unnormalized_obligations(
                    infcx,
                    key.param_env,
                    term,
                    cause.span,
                    cause.body_id,
                ) else {
                    coverage.reject("wf-generation-none");
                    return None;
                };
                if obligations.len() > 64 {
                    coverage.reject_count("generated-obligation-budget", obligations.len());
                    return None;
                }

                // Nominal bounds can introduce aliases and binders absent from the input.
                // Only skip normalization when every generated predicate is also eligible.
                // These children are handled before fulfillment's general overflow check.
                for obligation in &obligations {
                    let predicate = obligation.predicate;
                    if !predicate.kind().bound_vars().is_empty() {
                        coverage.reject_predicate(infcx.tcx, "generated-binder", predicate, flags);
                        return None;
                    }
                    if predicate.has_type_flags(flags) {
                        coverage.reject_predicate(infcx.tcx, "generated-flags", predicate, flags);
                        return None;
                    }
                    match predicate.kind().skip_binder() {
                        ty::PredicateKind::Clause(
                            ty::ClauseKind::RegionOutlives(_) | ty::ClauseKind::TypeOutlives(_),
                        )
                        | ty::PredicateKind::DynCompatible(_) => {}
                        _ if sizedness_fast_path(infcx.tcx, predicate, obligation.param_env) => {}
                        _ => {
                            coverage.reject_predicate(
                                infcx.tcx,
                                "unsupported-generated-predicate",
                                predicate,
                                flags,
                            );
                            return None;
                        }
                    }
                }

                // Dyn compatibility can run queries, so first reject every unsupported goal.
                for obligation in &obligations {
                    if let ty::PredicateKind::DynCompatible(def_id) =
                        obligation.predicate.kind().skip_binder()
                        && !infcx.tcx.is_dyn_compatible(def_id)
                    {
                        coverage.reject_predicate(
                            infcx.tcx,
                            "dyn-incompatible",
                            obligation.predicate,
                            flags,
                        );
                        return None;
                    }
                }
                for obligation in &obligations {
                    match obligation.predicate.kind().skip_binder() {
                        ty::PredicateKind::Clause(ty::ClauseKind::RegionOutlives(predicate)) => {
                            infcx.register_region_outlives_constraint(
                                predicate,
                                ty::VisibleForLeakCheck::Yes,
                                &obligation.cause,
                            );
                        }
                        ty::PredicateKind::Clause(ty::ClauseKind::TypeOutlives(
                            ty::OutlivesPredicate(ty, region),
                        )) => {
                            infcx.register_type_outlives_constraint(ty, region, &obligation.cause);
                        }
                        _ => {}
                    }
                }
            }
            _ => unreachable!(),
        }
        // Mark success only after the existing constraint registration has completed.
        coverage.accept(match clause {
            ty::ClauseKind::WellFormed(_) => "local-wf",
            ty::ClauseKind::TypeOutlives(_) => "direct-type-outlives",
            ty::ClauseKind::RegionOutlives(_) => "direct-region-outlives",
            _ => unreachable!(),
        });
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

/// Bounds the input traversal and excludes WF paths that inspect hidden types or emit diagnostics.
/// This only checks the input shape; `wf::unnormalized_obligations` supplies the WF requirements.
struct WfInput {
    remaining: u8,
}

enum InputFailure<'tcx> {
    NodeBudget,
    DynPredicateBudget { count: usize, remaining: u8 },
    AssociatedConst,
    Type(Ty<'tcx>),
    Constant(ty::Const<'tcx>),
}

impl WfInput {
    fn visit_node<'tcx>(&mut self) -> ControlFlow<InputFailure<'tcx>> {
        let Some(remaining) = self.remaining.checked_sub(1) else {
            return ControlFlow::Break(InputFailure::NodeBudget);
        };
        self.remaining = remaining;
        ControlFlow::Continue(())
    }
}

impl<'tcx> TypeVisitor<TyCtxt<'tcx>> for WfInput {
    type Result = ControlFlow<InputFailure<'tcx>>;

    fn visit_ty(&mut self, ty: Ty<'tcx>) -> Self::Result {
        self.visit_node()?;
        match ty.kind() {
            ty::Bool
            | ty::Char
            | ty::Int(_)
            | ty::Uint(_)
            | ty::Float(_)
            | ty::Str
            | ty::Never
            | ty::Param(_)
            | ty::Foreign(_)
            | ty::Adt(..)
            | ty::Tuple(_)
            | ty::Ref(..)
            | ty::RawPtr(..)
            | ty::Slice(_)
            | ty::Array(..)
            | ty::FnPtr(..) => {}
            ty::Dynamic(predicates, _) => {
                if predicates.len() > usize::from(self.remaining) {
                    return ControlFlow::Break(InputFailure::DynPredicateBudget {
                        count: predicates.len(),
                        remaining: self.remaining,
                    });
                }
                self.remaining -= predicates.len() as u8;
                // Associated const bindings inspect their declared types during WF generation.
                if predicates
                    .projection_bounds()
                    .any(|predicate| predicate.skip_binder().term.as_const().is_some())
                {
                    return ControlFlow::Break(InputFailure::AssociatedConst);
                }
            }
            _ => return ControlFlow::Break(InputFailure::Type(ty)),
        }
        ty.super_visit_with(self)
    }

    fn visit_region(&mut self, _region: ty::Region<'tcx>) -> Self::Result {
        self.visit_node()
    }

    fn visit_const(&mut self, constant: ty::Const<'tcx>) -> Self::Result {
        self.visit_node()?;
        match constant.kind() {
            ty::ConstKind::Param(_) => ControlFlow::Continue(()),
            // Aggregate values can inspect ADT fields with min_generic_const_args enabled.
            ty::ConstKind::Value(value)
                if matches!(
                    value.ty.kind(),
                    ty::Bool | ty::Char | ty::Int(_) | ty::Uint(_) | ty::Float(_)
                ) =>
            {
                constant.super_visit_with(self)
            }
            _ => ControlFlow::Break(InputFailure::Constant(constant)),
        }
    }
}
