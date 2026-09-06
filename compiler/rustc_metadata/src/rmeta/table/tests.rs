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

                // Prefix reuse copies a table's complete bytes to a different suffix position.
                let position = NonZero::new(37).unwrap();
                let mut relocated = vec![0; position.get()];
                relocated.extend_from_slice(&blob[table.encoded_range(&blob).unwrap()]);
                relocated.extend_from_slice(MAGIC_END_BYTES);
                let relocated =
                    MetadataBlob::new(slice_owned(relocated, |bytes| &bytes[..])).unwrap();
                let table = LazyTable::<usize, u64>::from_position_and_encoded_size(
                    position,
                    table.width,
                    len,
                );
                for (i, expected) in blocks.iter().enumerate() {
                    assert_eq!(table.get(&relocated, i), u64::from_le_bytes(*expected));
                }
            }
        }
    }
}

#[test]
fn invalid_table_layouts() {
    for (position, width, len) in [
        (1, 9, 1),
        (1, SPARSE_TABLE | 9, 1),
        (1, SPARSE_TABLE * 2 | 1, 1),
        (1, SPARSE_TABLE, 1),
        (1, SPARSE_TABLE | 1, 0),
        (1, 8, usize::MAX),
        (usize::MAX, 1, 1),
        (usize::MAX, SPARSE_TABLE | 1, 1),
    ] {
        let position = NonZero::new(position).unwrap();
        let table = LazyTable::<usize, u64> { position, width, len, _marker: PhantomData };
        assert!(table.checked_layout().is_none());
        assert!(table.encoded_range(&[0; 64]).is_none());
        assert!(
            std::panic::catch_unwind(|| {
                LazyTable::<usize, u64>::from_position_and_encoded_size(position, width, len)
            })
            .is_err()
        );
    }
}

#[test]
fn validate_every_sparse_bucket() {
    let blocks: Vec<_> =
        (0..129).map(|i| if i % SPARSE_BUCKET_LEN == 0 { [1u8] } else { [0] }).collect();
    let index = sparse_table_index(&blocks, 1).unwrap();
    let mut bytes = vec![0];
    bytes.extend_from_slice(index.as_slice().as_flattened());
    bytes.extend_from_slice(&[1; 3]);
    let table = LazyTable::<usize, u64>::from_position_and_encoded_size(
        NonZero::new(1).unwrap(),
        SPARSE_TABLE | 1,
        blocks.len(),
    );
    assert_eq!(table.encoded_range(&bytes), Some(1..bytes.len()));
    for end in 0..bytes.len() {
        assert!(table.encoded_range(&bytes[..end]).is_none());
    }

    for bucket in 0..index.len() {
        let mut invalid = bytes.clone();
        let rank = 1 + bucket * SPARSE_BUCKET_BYTES + 8;
        invalid[rank..rank + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(table.encoded_range(&invalid).is_none());
    }
    let mut invalid = bytes.clone();
    let last = 1 + (index.len() - 1) * SPARSE_BUCKET_BYTES;
    invalid[last..last + 8].copy_from_slice(&(1u64 << 63).to_le_bytes());
    assert!(table.encoded_range(&invalid).is_none());

    // A malformed rank cannot wrap an offset and decode an unrelated value during lookup.
    let mut invalid = bytes;
    invalid[9..13].copy_from_slice(&u32::MAX.to_le_bytes());
    invalid.extend_from_slice(MAGIC_END_BYTES);
    let blob = MetadataBlob::new(slice_owned(invalid, |bytes| &bytes[..])).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| table.get(&blob, 0))).is_err()
    );
}

#[test]
fn sparse_encoding_must_be_strictly_smaller() {
    let mut blocks = [[0u8]; 15];
    blocks[0] = [1];
    // 12 index bytes + one value byte + the extra width varint byte.
    assert!(sparse_table_index(&blocks[..14], 1).is_none());
    assert!(sparse_table_index(&blocks, 1).is_some());
}

#[test]
fn sparse_public_hashes() {
    let values: Vec<_> = (0..257)
        .map(|i| (i % 64 == 0).then(|| Fingerprint::from_le_bytes([i as u8 | 1; 16])))
        .collect();
    let blocks: Vec<_> = values
        .iter()
        .map(|&value| {
            let mut block = [0; 17];
            value.write_to_bytes(&mut block);
            block
        })
        .collect();
    let index = sparse_table_index(&blocks, 17).unwrap();
    let mut bytes = vec![0];
    bytes.extend_from_slice(index.as_slice().as_flattened());
    for block in blocks.iter().filter(|block| **block != [0; 17]) {
        bytes.extend_from_slice(block);
    }
    let end = bytes.len();
    bytes.extend_from_slice(MAGIC_END_BYTES);
    let blob = MetadataBlob::new(slice_owned(bytes, |bytes| &bytes[..])).unwrap();
    let table = LazyTable::<usize, Option<Fingerprint>>::from_position_and_encoded_size(
        NonZero::new(1).unwrap(),
        SPARSE_TABLE | 17,
        values.len(),
    );
    assert_eq!(table.encoded_range(&blob), Some(1..end));
    for (i, &value) in values.iter().enumerate() {
        assert_eq!(table.get(&blob, i), value);
    }
}
