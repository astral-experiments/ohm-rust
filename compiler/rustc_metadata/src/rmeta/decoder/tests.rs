use super::{CrateNum, DefPathHash, TraitImplDirectory, TraitPosting, TraitPostingRange};

#[test]
fn trait_impl_suppliers_in_crate_order() {
    let key = DefPathHash::default();
    let directory = TraitImplDirectory {
        ranges: [(key, TraitPostingRange { start: 0, general_end: 2, end: 5 })]
            .into_iter()
            .collect(),
        // Suppliers 1 and 5 may implement the trait for any receiver. The others own their receivers.
        postings: [1, 5, 2, 3, 7]
            .into_iter()
            .enumerate()
            .map(|(group, supplier)| TraitPosting {
                supplier: CrateNum::new(supplier),
                group: group as u32,
            })
            .collect(),
    };
    let visit = |owner: Option<usize>| {
        let mut visited = Vec::new();
        directory.visit(key, owner.map(CrateNum::new), |p| {
            visited.push((p.supplier.as_usize(), p.group))
        });
        visited
    };
    assert_eq!(visit(Some(3)), [(1, 0), (3, 3), (5, 1)]);
    assert_eq!(visit(Some(4)), [(1, 0), (5, 1)]);
    assert_eq!(visit(None), [(1, 0), (2, 2), (3, 3), (5, 1), (7, 4)]);
}
