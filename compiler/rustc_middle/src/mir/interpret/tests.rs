use rustc_abi::TargetDataLayout;
use rustc_data_structures::intern::Interned;

use super::{
    Align, AllocDecodingState, AllocMap, Allocation, Arc, CTFE_ALLOC_SALT, ConstAllocation,
    GlobalAlloc, Mutability, Scalar, Size, alloc_range, is_immutable_leaf,
};

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

#[test]
fn only_the_original_ctfe_literal_id_is_restored() {
    rustc_data_structures::sync::set_dyn_thread_safe_mode(true);
    let map = AllocMap::new();
    let bytes = Allocation::from_bytes_byte_aligned_immutable(&b"same"[..], ());
    let literal = ConstAllocation(Interned::new_unchecked(&bytes));
    let canonical = map.reserve();
    let salted = map.reserve();
    let distinct = map.reserve();
    map.dedup.lock().insert((GlobalAlloc::Memory(literal), CTFE_ALLOC_SALT), canonical);
    map.dedup.lock().insert((GlobalAlloc::Memory(literal), 1), salted);
    assert!(map.is_deduplicated_memory(canonical, literal));
    assert!(!map.is_deduplicated_memory(salted, literal));
    assert!(!map.is_deduplicated_memory(distinct, literal));

    let layout = TargetDataLayout::default();
    let size = layout.pointer_size();
    let mut pointer =
        Allocation::from_bytes(vec![0; size.bytes_usize()], Align::ONE, Mutability::Mut, ());
    assert!(!is_immutable_leaf(ConstAllocation(Interned::new_unchecked(&pointer))));
    pointer
        .write_scalar(
            &layout,
            alloc_range(Size::ZERO, size),
            Scalar::from_pointer(canonical.into(), &layout),
        )
        .unwrap();
    pointer.mutability = Mutability::Not;
    assert!(!is_immutable_leaf(ConstAllocation(Interned::new_unchecked(&pointer))));
}
