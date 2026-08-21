use super::*;
use crate::create_default_session_globals_then;

#[test]
fn cached_local_expansion_registration_is_idempotent() {
    create_default_session_globals_then(|| {
        let data = ExpnData::default(
            ExpnKind::AstPass(AstPass::TestHarness),
            DUMMY_SP,
            Edition::Edition2024,
            None,
            None,
        );
        let hash = ExpnHash::new(StableCrateId(Hash64::new(1)), Hash64::new(2));
        let previous_count = HygieneData::with(|hygiene_data| hygiene_data.local_expn_data.len());

        let first = register_local_expn_id(data.clone(), hash);
        let second = register_local_expn_id(data, hash);

        assert_eq!(first, second);
        assert_eq!(ExpnId::from_hash(hash), Some(first));
        HygieneData::with(|hygiene_data| {
            assert_eq!(hygiene_data.local_expn_data.len(), previous_count + 1);
            assert_eq!(hygiene_data.local_expn_hashes.len(), previous_count + 1);
        });
    });
}

#[test]
#[should_panic(expected = "cached local expansion hash is registered to a foreign expansion")]
fn cached_local_expansion_rejects_foreign_identity() {
    create_default_session_globals_then(|| {
        let data = ExpnData::default(
            ExpnKind::AstPass(AstPass::TestHarness),
            DUMMY_SP,
            Edition::Edition2024,
            None,
            None,
        );
        let hash = ExpnHash::new(StableCrateId(Hash64::new(1)), Hash64::new(2));

        register_expn_id(CrateNum::from_u32(1), ExpnIndex::from_u32(1), data.clone(), hash);
        register_local_expn_id(data, hash);
    });
}
