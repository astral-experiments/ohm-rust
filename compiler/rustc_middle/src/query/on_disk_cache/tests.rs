use super::{AbsoluteBytePos, DenseValues, SerializedDepNodeIndex};

fn entry(index: u32, pos: u64) -> (SerializedDepNodeIndex, AbsoluteBytePos) {
    (SerializedDepNodeIndex::from_u32(index), AbsoluteBytePos(pos))
}

#[test]
fn packed_indices_cross_word_boundaries() {
    let pairs = [
        entry(191, u32::MAX.into()),
        entry(1, 0),
        entry(128, 19),
        entry(63, 71),
        entry(0, 0x8765_4321),
    ];
    let ordered = DenseValues::pack(&pairs).unwrap();
    let values = DenseValues::new(ordered).unwrap();
    for i in 0..=192 {
        let index = SerializedDepNodeIndex::from_u32(i);
        let expected = pairs.iter().find(|&&(k, _)| k == index).map(|&(_, p)| p);
        assert_eq!(values.get(index), expected);
    }
}

#[test]
fn sparse_wide_and_repeated_indices_keep_pairs() {
    assert!(DenseValues::pack(&[]).is_none());
    assert!(DenseValues::pack(&[entry(0, 1), entry(1024, 2)]).is_none());
    assert!(DenseValues::pack(&[entry(0, 1), entry(1, 1u64 << 32)]).is_none());
    assert!(DenseValues::pack(&[entry(0, 1), entry(0, 2)]).is_none());

    let mut ordered = DenseValues::pack(&[entry(0, 1), entry(1, 2)]).unwrap();
    ordered.bytes.pop();
    assert!(DenseValues::new(ordered).is_err());
}
