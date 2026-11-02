use rustc_span::def_id::StableCrateId;
use rustc_span::sym;

use super::{DefPathData, Definitions, FIRST_DERIVED_IMPL, PerParentDisambiguatorState};
#[test]
fn a_derived_slot_reserves_the_dense_ordinal_and_preserves_full_hash_collision_errors() {
    rustc_span::create_default_session_globals_then(|| {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        use rustc_span::def_id::CRATE_DEF_ID;

        let name = sym::test;
        let mut defs = Definitions::new(StableCrateId::new(name, false, vec![], "test"));
        let mut state = PerParentDisambiguatorState::new(CRATE_DEF_ID);
        let high = FIRST_DERIVED_IMPL + 7;
        let hinted = defs.create_def_with_derive_slot(
            CRATE_DEF_ID,
            DefPathData::Impl,
            &mut state,
            Some(high),
        );
        let normal =
            defs.create_def_with_derive_slot(CRATE_DEF_ID, DefPathData::Impl, &mut state, None);
        assert_eq!(defs.def_key(normal).disambiguated_data.disambiguator, 1);
        let occupied = defs.create_def_with_derive_slot(
            CRATE_DEF_ID,
            DefPathData::Impl,
            &mut state,
            Some(high),
        );
        assert_eq!(defs.def_key(occupied).disambiguated_data.disambiguator, 2);
        assert_ne!(defs.def_key(occupied), defs.def_key(hinted));
        state.next.insert(DefPathData::Impl, high);
        let next =
            defs.create_def_with_derive_slot(CRATE_DEF_ID, DefPathData::Impl, &mut state, None);
        assert_eq!(defs.def_key(next).disambiguated_data.disambiguator, high + 1);
        let hash = defs.def_path_hash(hinted);
        let error = catch_unwind(AssertUnwindSafe(|| defs.try_allocate(defs.def_key(next), hash)));
        assert!(
            error
                .unwrap_err()
                .downcast_ref::<String>()
                .unwrap()
                .contains("found DefPathHash collision")
        );
        assert_eq!(defs.local_def_path_hash_to_def_id(hash), Some(hinted));
    });
}
