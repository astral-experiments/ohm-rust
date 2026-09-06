use rustc_data_structures::owned_slice::slice_owned;
use rustc_serialize::opaque::MAGIC_END_BYTES;

use super::*;

#[test]
fn sparse_and_dense_tables() {
    for width in [1, 2, 8] {
        for len in [0, 1, 63, 64, 65, 127, 128, 129, 257] {
            for sparse in [false, true] {
                let blocks: Vec<_> = (0..len)
                    .map(|i| {
                        let value = if !sparse || [0, 63, 64, 127, 128, 256].contains(&i) {
                            1u64 << ((width - 1) * 8)
                        } else {
                            0
                        };
                        value.to_le_bytes()
                    })
                    .collect();
                let index = sparse_table_index(&blocks, width);
                if !sparse {
                    assert!(index.is_none());
                } else if len == 257 {
                    assert!(index.is_some());
                }
                let mut encoded = vec![0];
                if let Some(index) = &index {
                    encoded.extend(index.iter().flatten());
                }
                for block in &blocks {
                    if index.is_none() || *block != [0; 8] {
                        encoded.extend_from_slice(&block[..width]);
                    }
                }
                let encoded_end = encoded.len();
                encoded.extend_from_slice(MAGIC_END_BYTES);
                let blob = MetadataBlob::new(slice_owned(encoded, |bytes| &bytes[..])).unwrap();
                let table = LazyTable::<usize, u64>::from_position_and_encoded_size(
                    NonZero::new(1).unwrap(),
                    width | if index.is_some() { SPARSE_TABLE } else { 0 },
                    len,
                );
                assert_eq!(table.encoded_range(&blob), Some(1..encoded_end));
                assert_eq!(table.encoded_range(&blob[..encoded_end - 1]), None);
                for (i, expected) in blocks.iter().enumerate() {
                    assert_eq!(table.get(&blob, i), u64::from_le_bytes(*expected));
                }
                assert_eq!(table.get(&blob, len), 0);
                assert_eq!(table.get(&blob, len + 100), 0);
            }
        }
    }
}
