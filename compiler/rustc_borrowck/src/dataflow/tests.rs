use rustc_index::bit_set::MixedBitSet;

use super::BorrowckDomain;

#[test]
fn clone_from_replaces_bits_and_handles_domain_changes() {
    let empty = |[borrows, uninits, ever_inits]: [usize; 3]| BorrowckDomain {
        borrows: MixedBitSet::new_empty(borrows),
        uninits: MixedBitSet::new_empty(uninits),
        ever_inits: MixedBitSet::new_empty(ever_inits),
    };
    let mut state = empty([0, 0, 0]);
    for sizes in [[1, 65, 4096], [4096, 1, 65], [0, 0, 0]] {
        let mut filled = empty(sizes);
        filled.borrows.insert_all();
        filled.uninits.insert_all();
        filled.ever_inits.insert_all();
        state.clone_from(&filled);
        assert_eq!(state, filled);
        let cleared = empty(sizes);
        state.clone_from(&cleared);
        assert_eq!(state, cleared);
    }
}
