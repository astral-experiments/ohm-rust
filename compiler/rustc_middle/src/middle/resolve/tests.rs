use super::*;

#[test]
fn resolver_slots_round_trip_early_local_and_synthetic_ids() {
    let node = NodeId::from_u32;
    let mut table = PartialResolutions::default();
    let local = PartialRes::new(Res::Local(node(0)));
    table.insert(node(3), local);
    table.insert(ast::DUMMY_NODE_ID, local);
    table.prepare(node(4));
    assert!(!table.contains_key(node(0)));
    assert!(table.values.is_empty());
    assert_eq!(table.get(node(3)).unwrap().expect_full_res(), Res::Local(node(0)));
    assert_eq!(table.get(ast::DUMMY_NODE_ID).unwrap().expect_full_res(), Res::Local(node(0)));

    let wide = PartialRes::new(Res::Local(node(1 << 31)));
    let partial = PartialRes::with_unresolved_segments(Res::Local(node(0)), 2);
    for id in [node(0), node(3), node(4), node(1 << 31), ast::DUMMY_NODE_ID] {
        table.insert(id, local);
        assert_eq!(table.insert(id, wide).unwrap().expect_full_res(), local.expect_full_res());
        assert_eq!(table.insert(id, partial).unwrap().expect_full_res(), wide.expect_full_res());
        assert_eq!(table.get(id).unwrap().unresolved_segments(), 2);
        assert!(table.insert(id, local).unwrap().full_res().is_none());
        assert_eq!(table.get(id).unwrap().expect_full_res(), local.expect_full_res());
    }
    assert_eq!(table.slots.len(), 4);
    assert_eq!(table.values.len(), 2);
}

#[test]
fn resolver_slots_share_only_equal_full_locals() {
    let node = NodeId::from_u32;
    let mut table = PartialResolutions::default();
    table.prepare(node(8));
    table.insert(node(3), PartialRes::new(Res::Local(node(1))));
    table.insert(node(4), PartialRes::new(Res::Local(node(1))));
    assert_eq!(table.shared_local(node(3), node(4)), Some(node(1)));
    table.insert(node(4), PartialRes::new(Res::Local(node(2))));
    assert_eq!(table.shared_local(node(3), node(4)), None);
    table.insert(node(4), PartialRes::with_unresolved_segments(Res::Local(node(1)), 2));
    assert_eq!(table.shared_local(node(3), node(4)), None);
    assert_eq!(table.shared_local(node(0), node(1)), None);
    assert_eq!(table.shared_local(node(9), node(10)), None);
}
