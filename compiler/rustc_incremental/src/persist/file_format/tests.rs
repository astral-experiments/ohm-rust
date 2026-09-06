use std::os::unix::fs::MetadataExt;

use super::*;

#[test]
fn cloned_prefix_keeps_previous_session_immutable() {
    let temp = rustc_fs_util::TempDirBuilder::new().tempdir_in(env::temp_dir()).unwrap();
    let path = temp.path().join("query-cache.bin");
    let donor = temp.path().join("previous-session.bin");
    let prefix = vec![42; 128 * 1024 + 3];
    fs::write(&donor, &prefix).unwrap();
    fs::hard_link(&donor, &path).unwrap();

    let mut encoder = match cloned_prefix_encoder(&path, prefix.len()) {
        Err(err) if err.kind() == io::ErrorKind::Unsupported => return,
        result => result.unwrap(),
    };
    assert_ne!(fs::metadata(&path).unwrap().ino(), fs::metadata(&donor).unwrap().ino());
    encoder.emit_raw_bytes(b"next generation");
    encoder.finish().unwrap();
    assert_eq!(fs::read(&donor).unwrap(), prefix);
    assert_eq!(&fs::read(&path).unwrap()[..prefix.len()], prefix);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
}

#[test]
fn failed_prefix_setup_preserves_previous_session_and_cleans_temporary_files() {
    let temp = rustc_fs_util::TempDirBuilder::new().tempdir_in(env::temp_dir()).unwrap();
    let path = temp.path().join("query-cache.bin");
    let donor = temp.path().join("previous-session.bin");
    fs::write(&donor, b"prefix").unwrap();
    fs::hard_link(&donor, &path).unwrap();
    assert!(cloned_prefix_encoder(&path, 1).is_err());
    assert_eq!(fs::read(&donor).unwrap(), b"prefix");
    assert_eq!(fs::read(&path).unwrap(), b"prefix");
    assert_eq!(fs::metadata(&path).unwrap().ino(), fs::metadata(&donor).unwrap().ino());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
}
