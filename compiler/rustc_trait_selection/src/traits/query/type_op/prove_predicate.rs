use rustc_infer::traits::Obligation;
use rustc_middle::traits::ObligationCause;
use rustc_middle::traits::query::NoSolution;
pub use rustc_middle::traits::query::type_op::ProvePredicate;
use rustc_middle::ty::{
    self, ParamEnvAnd, TyCtxt, TypeFlags, TypeVisitableExt, TypingModeEqWrapper,
};
use rustc_span::Span;

use crate::infer::InferCtxt;
use crate::infer::canonical::{CanonicalQueryInput, CanonicalQueryResponse, OriginalQueryValues};
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

    fn canonicalize_query(
        query_key: ParamEnvAnd<'tcx, Self>,
        infcx: &InferCtxt<'tcx>,
    ) -> (CanonicalQueryInput<'tcx, ParamEnvAnd<'tcx, Self>>, OriginalQueryValues<'tcx>) {
        // Check both the predicate and its environment. With these inputs, canonicalization
        // only records free-region identities in ROOT and never consults inference state.
        let disallowed = (TypeFlags::HAS_INFER - TypeFlags::HAS_RE_INFER)
            | TypeFlags::HAS_PLACEHOLDER
            | TypeFlags::HAS_TY_FRESH
            | TypeFlags::HAS_CT_FRESH
            | TypeFlags::HAS_ERROR
            | TypeFlags::HAS_BOUND_VARS;
        let cacheable = !infcx.disable_trait_solver_fast_paths()
            && !query_key.has_type_flags(disallowed)
            // Otherwise the predicate already takes the canonicalizer's trivial fast path.
            && query_key.value.predicate.has_type_flags(TypeFlags::HAS_FREE_REGIONS);
        let cache = &infcx.tcx.canonical_prove_predicate_cache;
        if cacheable {
            // Release the lock before any canonicalization, including the debug check.
            let cached = { cache.borrow().get(&query_key).cloned() };
            if let Some((canonical, original_values)) = cached {
                if cfg!(debug_assertions) {
                    let mut checked_values = OriginalQueryValues::default();
                    let checked = infcx.canonicalize_query(query_key, &mut checked_values);
                    assert_eq!(checked.canonical, canonical);
                    assert_eq!(checked_values.var_values, original_values.var_values);
                    assert_eq!(checked_values.universe_map, original_values.universe_map);
                }
                // The typing mode belongs to the caller, rather than the cached structure.
                let query = CanonicalQueryInput {
                    canonical,
                    typing_mode: TypingModeEqWrapper(infcx.typing_mode_raw()),
                };
                return (query, original_values);
            }
        }

        let mut original_values = OriginalQueryValues::default();
        let canonical = infcx.canonicalize_query(query_key, &mut original_values);
        if cacheable {
            // Another worker may have inserted the same pure result while we canonicalized.
            cache
                .borrow()
                .entry(query_key)
                .or_insert_with(|| (canonical.canonical, original_values.clone()));
        }
        (canonical, original_values)
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
