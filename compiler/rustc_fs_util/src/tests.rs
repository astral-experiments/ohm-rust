use std::os::unix::fs::{MetadataExt, PermissionsExt};

use super::*;

#[test]
fn cloned_directory_has_independent_files() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("source");
    let dst = temp.path().join("destination");
    fs::create_dir_all(src.join("nested/empty")).unwrap();
    fs::write(src.join("nested/artifact"), b"original").unwrap();
    fs::set_permissions(src.join("nested/artifact"), fs::Permissions::from_mode(0o755)).unwrap();

    match clone_directory(&src, &dst) {
        Err(err) if err.kind() == io::ErrorKind::Unsupported => return,
        result => result.unwrap(),
    }

    assert!(dst.join("nested/empty").is_dir());
    let source = src.join("nested/artifact");
    let cloned = dst.join("nested/artifact");
    assert_ne!(source.metadata().unwrap().ino(), cloned.metadata().unwrap().ino());
    assert_eq!(cloned.metadata().unwrap().permissions().mode() & 0o777, 0o755);
    fs::write(&cloned, b"changed clone").unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"original");
    fs::write(&source, b"changed source").unwrap();
    assert_eq!(fs::read(&cloned).unwrap(), b"changed clone");
}

#[test]
fn clone_does_not_replace_existing_destination() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("source");
    let dst = temp.path().join("destination");
    fs::create_dir(&src).unwrap();
    fs::create_dir(&dst).unwrap();
    fs::write(src.join("artifact"), b"source").unwrap();
    fs::write(dst.join("sentinel"), b"destination").unwrap();

    assert!(clone_directory(&src, &dst).is_err());
    assert_eq!(fs::read(dst.join("sentinel")).unwrap(), b"destination");
    assert!(!dst.join("artifact").exists());
}

#[test]
fn failed_clone_leaves_destination_absent() {
    let temp = tempfile::tempdir().unwrap();
    let dst = temp.path().join("destination");
    assert!(clone_directory(&temp.path().join("missing"), &dst).is_err());
    assert!(!dst.exists());
}
