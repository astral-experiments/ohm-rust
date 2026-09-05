use rustc_infer::traits::{Obligation, PredicateObligations};
use rustc_middle::traits::ObligationCause;
use rustc_middle::traits::query::NoSolution;
pub use rustc_middle::traits::query::type_op::ProvePredicate;
use rustc_middle::ty::{self, ParamEnvAnd, TyCtxt};
use rustc_span::Span;

use crate::infer::canonical::{
    CanonicalQueryInput, CanonicalQueryResponse, Certainty, OriginalQueryValues,
    QueryRegionConstraints,
};
use crate::infer::{InferCtxt, InferOk};
use crate::traits::{ObligationCtxt, sizedness_fast_path};

pub(super) mod proof_timing;

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

    fn perform_query(
        tcx: TyCtxt<'tcx>,
        canonicalized: CanonicalQueryInput<'tcx, ParamEnvAnd<'tcx, Self>>,
    ) -> Result<CanonicalQueryResponse<'tcx, ()>, NoSolution> {
        tcx.type_op_prove_predicate(canonicalized)
    }

    fn fully_perform_into(
        query_key: ParamEnvAnd<'tcx, Self>,
        infcx: &InferCtxt<'tcx>,
        output_query_region_constraints: &mut QueryRegionConstraints<'tcx>,
        span: Span,
    ) -> Result<
        (
            Self::QueryResponse,
            Option<CanonicalQueryInput<'tcx, ParamEnvAnd<'tcx, Self>>>,
            PredicateObligations<'tcx>,
            Certainty,
        ),
        NoSolution,
    > {
        let _timing = proof_timing::start_prove(infcx.tcx, &query_key);
        let mut canonical_var_values = OriginalQueryValues::default();
        let old_param_env = query_key.param_env;
        let canonical_self = infcx.canonicalize_query(query_key, &mut canonical_var_values);
        let canonical_result = Self::perform_query(infcx.tcx, canonical_self)?;

        let InferOk { value, obligations } = infcx
            .instantiate_nll_query_response_and_region_obligations(
                &ObligationCause::dummy_with_span(span),
                old_param_env,
                &canonical_var_values,
                canonical_result,
                output_query_region_constraints,
            )?;

        Ok((value, Some(canonical_self), obligations, canonical_result.value.certainty))
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
