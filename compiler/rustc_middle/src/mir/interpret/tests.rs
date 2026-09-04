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
}
