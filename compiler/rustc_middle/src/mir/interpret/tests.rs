use super::{AllocDecodingState, Arc};

#[test]
fn aliases_preserve_allocation_identity() {
    let original = Arc::new(AllocDecodingState::new(vec![10]));
    let rewritten = Arc::new(
        AllocDecodingState::new(vec![20, 30])
            .with_aliases(vec![Some((Arc::clone(&original), 0)), None]),
    );
    let rewritten_again =
        AllocDecodingState::new(vec![40]).with_aliases(vec![Some((Arc::clone(&rewritten), 0))]);

    assert!(std::ptr::eq(original.state_at(0), rewritten.state_at(0)));
    assert!(std::ptr::eq(original.state_at(0), rewritten_again.state_at(0)));
    assert!(!std::ptr::eq(original.state_at(0), rewritten.state_at(1)));
    let session = rewritten_again.new_decoding_session();
    let (origin, index) = session.alias(0).unwrap();
    assert!(std::ptr::eq(origin, &*rewritten));
    assert_eq!(index, 0);
    let (origin, index) = origin.new_decoding_session().alias(index).unwrap();
    assert!(std::ptr::eq(origin, &*original));
    assert_eq!(index, 0);
    assert!(origin.new_decoding_session().alias(index).is_none());
}
