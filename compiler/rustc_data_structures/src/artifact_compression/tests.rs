use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestFile(PathBuf);

impl TestFile {
    fn new(bytes: &[u8]) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("rust-artifact-compression-test-{}-{id}.bin", std::process::id()));
        fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for TestFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn repetitive_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| b'a' + ((i / 1024) % 5) as u8).collect()
}

#[test]
fn crc32_slicing_by_eight_matches_reference_for_tails_and_alignments() {
    assert_eq!(crc32(b"123456789"), 0xcbf4_3926);

    let mut state = 0x1234_5678u32;
    let bytes = (0..DEFAULT_CHUNK_SIZE + 8)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect::<Vec<_>>();
    let lengths = [
        0,
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        15,
        16,
        17,
        31,
        32,
        33,
        63,
        64,
        65,
        255,
        256,
        257,
        DEFAULT_CHUNK_SIZE - 1,
        DEFAULT_CHUNK_SIZE,
        DEFAULT_CHUNK_SIZE + 1,
    ];
    for offset in 0..8 {
        for len in lengths {
            let slice = &bytes[offset..offset + len];
            assert_eq!(crc32(slice), crc32_bytewise(slice), "offset={offset}, len={len}");
        }
    }
}

#[cfg(target_arch = "aarch64")]
#[test]
fn crc32_hardware_matches_portable_when_supported() {
    if !std::arch::is_aarch64_feature_detected!("crc") {
        return;
    }
    let mut state = 0xa5a5_1234u32;
    let bytes = (0..DEFAULT_CHUNK_SIZE + 17)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect::<Vec<_>>();
    for offset in 0..8 {
        for len in [0, 1, 2, 3, 4, 7, 8, 9, 31, 1024, DEFAULT_CHUNK_SIZE + 1] {
            let slice = &bytes[offset..offset + len];
            assert_eq!(
                unsafe { crc32_aarch64(slice) },
                crc32_slicing_by_eight(slice),
                "offset={offset}, len={len}"
            );
        }
    }
}

fn crc32_bytewise(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        let index = ((crc ^ u32::from(*byte)) & 0xff) as usize;
        crc = CRC32_TABLES[0][index] ^ (crc >> 8);
    }
    !crc
}

#[test]
fn round_trip_pack_and_unpack() {
    let bytes = repetitive_bytes(DEFAULT_CHUNK_SIZE * 3 + 17);
    let source = TestFile::new(&bytes);
    let destination = TestFile::new(b"old destination");

    let stats = pack(&source.0).unwrap();
    assert!(stats.compressed);
    assert_eq!(stats.original_bytes, bytes.len() as u64);
    assert!(stats.packed_bytes < stats.original_bytes);
    assert!(is_compressed(&source.0).unwrap());
    assert_eq!(read_all(&source.0).unwrap(), bytes);

    unpack_to(&source.0, &destination.0).unwrap();
    assert_eq!(fs::read(&destination.0).unwrap(), bytes);
    assert!(is_compressed(&source.0).unwrap());
    assert_eq!(pack(&source.0).unwrap(), stats);
}

#[test]
fn configurable_chunks_round_trip_and_preserve_range_reads() {
    let chunk_size = 256 * 1024;
    let bytes = repetitive_bytes(chunk_size * 2 + 19);
    let file = TestFile::new(&bytes);
    let options = CompressionOptions { level: 3, chunk_size };

    let stats = pack_with_options(&file.0, options).unwrap();
    assert!(stats.compressed);
    let container = parse_container(&file.0).unwrap();
    assert_eq!(container.chunk_size, chunk_size);
    assert_eq!(container.entries.len(), 3);
    assert_eq!(read_all(&file.0).unwrap(), bytes);
    for (offset, len) in [
        (0, 9),
        ((chunk_size - 3) as u64, 11),
        ((chunk_size + 41) as u64, 999),
        ((bytes.len() - 7) as u64, 7),
    ] {
        assert_eq!(
            read_range(&file.0, offset, len).unwrap(),
            bytes[offset as usize..offset as usize + len]
        );
    }

    let destination = TestFile::new(b"old destination");
    unpack_to(&file.0, &destination.0).unwrap();
    assert_eq!(fs::read(&destination.0).unwrap(), bytes);
    assert_eq!(pack_with_options(&file.0, CompressionOptions::default()).unwrap(), stats);
}

#[test]
fn chunk_size_options_are_validated_before_writing() {
    let file = TestFile::new(&repetitive_bytes(DEFAULT_CHUNK_SIZE * 2));
    let original = fs::read(&file.0).unwrap();
    for chunk_size in [0, 8 * 1024, 24 * 1024, MAX_CHUNK_SIZE * 2] {
        let error =
            pack_with_options(&file.0, CompressionOptions { level: 3, chunk_size }).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(fs::read(&file.0).unwrap(), original);
    }
}

#[test]
fn files_no_larger_than_container_overhead_skip_compression() {
    for len in [0, MINIMUM_PACKABLE_SIZE as usize] {
        let bytes = repetitive_bytes(len);
        let file = TestFile::new(&bytes);
        let stats = pack(&file.0).unwrap();
        assert!(!stats.compressed);
        assert_eq!(stats.original_bytes, len as u64);
        assert_eq!(fs::read(&file.0).unwrap(), bytes);
    }
}

#[test]
fn range_reads_only_the_requested_bytes() {
    let bytes = repetitive_bytes(DEFAULT_CHUNK_SIZE * 4 + 91);
    let file = TestFile::new(&bytes);
    assert!(pack(&file.0).unwrap().compressed);

    for (offset, len) in [
        (0, 1),
        ((DEFAULT_CHUNK_SIZE - 2) as u64, 9),
        ((DEFAULT_CHUNK_SIZE * 2 + 101) as u64, 700),
        ((bytes.len() - 33) as u64, 33),
        (bytes.len() as u64, 0),
    ] {
        assert_eq!(
            read_range(&file.0, offset, len).unwrap(),
            bytes[offset as usize..offset as usize + len]
        );
    }
}

#[test]
fn mixed_chunks_use_raw_fallback_and_round_trip() {
    let mut bytes = Vec::with_capacity(DEFAULT_CHUNK_SIZE * 2);
    let mut state = 0x1234_5678u32;
    for _ in 0..DEFAULT_CHUNK_SIZE {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        bytes.push(state as u8);
    }
    bytes.extend(std::iter::repeat_n(b'R', DEFAULT_CHUNK_SIZE));
    let file = TestFile::new(&bytes);

    let stats = pack(&file.0).unwrap();
    assert!(stats.compressed);
    let container = parse_container(&file.0).unwrap();
    assert_eq!(container.entries.len(), 2);
    assert_eq!(container.entries[0].flags, RAW_CHUNK);
    assert_eq!(container.entries[1].flags, 0);
    assert_eq!(read_all(&file.0).unwrap(), bytes);
}

#[test]
fn range_reads_validate_only_chunks_they_touch() {
    let bytes = repetitive_bytes(DEFAULT_CHUNK_SIZE * 2);
    let file = TestFile::new(&bytes);
    assert!(pack(&file.0).unwrap().compressed);

    flip_byte(&file.0, HEADER_LEN + 1);
    assert_eq!(
        read_range(&file.0, DEFAULT_CHUNK_SIZE as u64, 1).unwrap(),
        bytes[DEFAULT_CHUNK_SIZE..DEFAULT_CHUNK_SIZE + 1]
    );
    assert_eq!(read_range(&file.0, 0, 1).unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn malformed_truncated_and_corrupt_files_are_rejected() {
    let original = repetitive_bytes(DEFAULT_CHUNK_SIZE * 2);
    let truncated = TestFile::new(&original);
    assert!(pack(&truncated.0).unwrap().compressed);
    let length = fs::metadata(&truncated.0).unwrap().len();
    OpenOptions::new().write(true).open(&truncated.0).unwrap().set_len(length - 3).unwrap();
    assert_eq!(read_all(&truncated.0).unwrap_err().kind(), io::ErrorKind::InvalidData);

    let header_corrupt = TestFile::new(&original);
    pack(&header_corrupt.0).unwrap();
    flip_byte(&header_corrupt.0, 24);
    assert_eq!(read_all(&header_corrupt.0).unwrap_err().kind(), io::ErrorKind::InvalidData);

    let chunk_corrupt = TestFile::new(&original);
    pack(&chunk_corrupt.0).unwrap();
    flip_byte(&chunk_corrupt.0, HEADER_LEN + 1);
    assert_eq!(read_all(&chunk_corrupt.0).unwrap_err().kind(), io::ErrorKind::InvalidData);

    let length_corrupt = TestFile::new(&original);
    pack(&length_corrupt.0).unwrap();
    let mut header = [0; HEADER_LEN as usize];
    let mut file = OpenOptions::new().read(true).write(true).open(&length_corrupt.0).unwrap();
    file.read_exact(&mut header).unwrap();
    put_u64(&mut header, 24, 1);
    put_u32(&mut header, HEADER_CRC_OFFSET, 0);
    let header_checksum = crc32(&header);
    put_u32(&mut header, HEADER_CRC_OFFSET, header_checksum);
    file.seek(SeekFrom::Start(0)).unwrap();
    file.write_all(&header).unwrap();
    assert_eq!(read_all(&length_corrupt.0).unwrap_err().kind(), io::ErrorKind::InvalidData);

    let index_corrupt = TestFile::new(&original);
    pack(&index_corrupt.0).unwrap();
    let last_byte = fs::metadata(&index_corrupt.0).unwrap().len() - 1;
    flip_byte(&index_corrupt.0, last_byte);
    assert_eq!(read_all(&index_corrupt.0).unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn no_savings_leave_original_file_untouched() {
    let mut state = 0x8765_4321u32;
    let bytes = (0..DEFAULT_CHUNK_SIZE)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect::<Vec<_>>();
    let file = TestFile::new(&bytes);
    let stats = pack(&file.0).unwrap();
    assert!(!stats.compressed);
    assert_eq!(stats.original_bytes, bytes.len() as u64);
    assert_eq!(fs::read(&file.0).unwrap(), bytes);
}

#[test]
fn packing_and_unpacking_preserve_modification_time() {
    let bytes = repetitive_bytes(DEFAULT_CHUNK_SIZE * 2);
    let packed = TestFile::new(&bytes);
    let unpacked = TestFile::new(b"old");
    let expected = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_234_567_890);
    File::options()
        .write(true)
        .open(&packed.0)
        .unwrap()
        .set_times(FileTimes::new().set_modified(expected))
        .unwrap();

    assert!(pack(&packed.0).unwrap().compressed);
    assert_eq!(fs::metadata(&packed.0).unwrap().modified().unwrap(), expected);
    unpack_to(&packed.0, &unpacked.0).unwrap();
    assert_eq!(fs::metadata(&unpacked.0).unwrap().modified().unwrap(), expected);
}

fn flip_byte(path: &Path, offset: u64) {
    let mut file = OpenOptions::new().read(true).write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    let mut byte = [0];
    file.read_exact(&mut byte).unwrap();
    byte[0] ^= 0x80;
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(&byte).unwrap();
}
