use super::*;
use crate::artifact_compression::{CompressionOptions, pack_with_options};
use crate::owned_slice::slice_owned;
#[test]
fn decodes_only_requested_chunks_and_pins_cross_chunk_ranges() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("metadata");
    let raw: Vec<u8> = (0..65536).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &raw).unwrap();
    pack_with_options(&path, CompressionOptions { level: 3, chunk_size: 16384 }).unwrap();
    let source = slice_owned(std::fs::read(&path).unwrap(), |x| &x[..]);
    let reader = IndexedArtifact::from_slice(source).unwrap().unwrap();
    assert_eq!(reader.chunks.iter().filter(|c| c.get().is_some()).count(), 0);
    assert_eq!(reader.range(100, 20), &raw[100..120]);
    assert_eq!(reader.chunks.iter().filter(|c| c.get().is_some()).count(), 1);
    let crossing = reader.range(16380, 20);
    assert_eq!(crossing, &raw[16380..16400]);
    assert!(std::ptr::eq(crossing, reader.range(16380, 20)));
    assert_eq!(reader.chunks.iter().filter(|c| c.get().is_some()).count(), 2);
}
