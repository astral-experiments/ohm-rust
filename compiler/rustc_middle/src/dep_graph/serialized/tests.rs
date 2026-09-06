use super::*;

#[test]
fn compact_headers_preserve_indices_fingerprints_and_edge_layout() {
    let node = DepNode {
        kind: DepKind::SideEffect,
        key_fingerprint: Fingerprint::from_le_bytes([0xab; 16]).into(),
    };
    for index in [0, 255, 0x7fff_fffe] {
        for fingerprint in [Fingerprint::ZERO, Fingerprint::from_le_bytes([0xcd; 16])] {
            for (edge_max, edge_count) in [
                (0, 0),
                (255, SerializedNodeHeader::MAX_INLINE_LEN),
                (256, SerializedNodeHeader::MAX_INLINE_LEN + 1),
                (0x10000, 1000),
                (0x1000000, 1000),
            ] {
                let header = SerializedNodeHeader::new(
                    &node,
                    DepNodeIndex::from_u32(index),
                    fingerprint,
                    edge_max,
                    edge_count,
                );
                let expected_len = if fingerprint == Fingerprint::ZERO { 22 } else { 38 };
                assert_eq!(header.encoded_len(), expected_len);
                let mut encoder = MemEncoder::new();
                encoder.emit_raw_bytes(&header.bytes[..header.encoded_len()]);
                encoder.emit_u8(0x5a);
                encoder.emit_raw_bytes(rustc_serialize::opaque::MAGIC_END_BYTES);
                let mut decoder = MemDecoder::new(&encoder.data, 0).unwrap();
                let decoded = SerializedNodeHeader::decode(&mut decoder);
                assert_eq!(decoded.index().as_u32(), index);
                assert_eq!(decoded.node(), node);
                assert_eq!(decoded.value_fingerprint(), fingerprint);
                assert_eq!(decoded.len(), header.len());
                assert_eq!(decoded.bytes_per_index(), header.bytes_per_index());
                assert_eq!(decoder.read_u8(), 0x5a);
            }
        }
    }
}
