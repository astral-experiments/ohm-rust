use super::{pack, upstream_order};

#[test]
fn upstream_order_matches_full_sort() {
    let mut rows: Vec<_> =
        (0..256).rev().map(|i| ((((i / 64) << 56 | (i / 16), i / 4), i % 4), i)).collect();
    let order = upstream_order(&rows).unwrap();
    let ordered: Vec<_> = order.into_iter().map(|i| rows[i as usize]).collect();
    rows.sort_unstable_by_key(|row| row.0);
    assert_eq!(ordered, rows);
}

#[test]
fn exact_key_ties_use_the_original_sort() {
    let mut rows: Vec<_> = (0..256).map(|i| (((i << 56, 0), 0), i)).collect();
    assert!(upstream_order(&rows).is_some());
    rows[0].0 = rows[255].0;
    assert!(upstream_order(&rows).is_none());
}

#[test]
fn packed_address_uses_crate_list_position() {
    for crates in [1, 2, 17, 512] {
        for slot in 0..crates {
            for ordinal in [0, 1, 16_383] {
                let address = pack(slot, ordinal, crates) as usize;
                assert_eq!(address % crates, slot);
                assert_eq!(address / crates, ordinal);
            }
        }
    }
}

#[test]
fn overflow_falls_back_instead_of_aliasing_another_export() {
    let max = u32::MAX as usize;
    assert_eq!(pack(0, max - 1, 1), u32::MAX - 1);
    assert_eq!(pack(0, max, 1), u32::MAX);
    assert_eq!(pack(1, max, 2), u32::MAX);
    assert_eq!(pack(0, usize::MAX, 2), u32::MAX);
}
