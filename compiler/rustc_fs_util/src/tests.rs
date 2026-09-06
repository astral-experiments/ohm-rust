use std::os::unix::fs::MetadataExt;
#[cfg(target_os = "macos")]
use std::os::unix::fs::PermissionsExt;

use super::*;

#[test]
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
fn failed_clone_leaves_destination_absent() {
    let temp = tempfile::tempdir().unwrap();
    let dst = temp.path().join("destination");
    assert!(clone_directory(&temp.path().join("missing"), &dst).is_err());
    assert!(!dst.exists());
}

#[test]
fn cloned_file_can_append_without_changing_its_donor_or_hard_links() {
    use std::io::Write;

    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let old_session = temp.path().join("old-session");
    let cloned = temp.path().join("clone");
    let prefix = vec![42; 128 * 1024 + 3];
    fs::write(&source, &prefix).unwrap();
    fs::hard_link(&source, &old_session).unwrap();
    match clone_file(&source, &cloned) {
        Err(err) if err.kind() == io::ErrorKind::Unsupported => return,
        result => result.unwrap(),
    }
    assert_ne!(source.metadata().unwrap().ino(), cloned.metadata().unwrap().ino());
    fs::rename(&cloned, &source).unwrap();
    fs::File::options().append(true).open(&source).unwrap().write_all(b"new generation").unwrap();
    assert_eq!(fs::read(&old_session).unwrap(), prefix);
    assert_eq!(&fs::read(&source).unwrap()[prefix.len()..], b"new generation");
    fs::write(&old_session, b"changed donor").unwrap();
    assert_eq!(&fs::read(&source).unwrap()[..prefix.len()], prefix);
}

#[test]
fn failed_file_clone_preserves_existing_destination() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::write(&source, b"source").unwrap();
    fs::write(&destination, b"destination").unwrap();
    assert!(clone_file(&source, &destination).is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"destination");
    let absent = temp.path().join("absent");
    assert!(clone_file(&temp.path().join("missing"), &absent).is_err());
    assert!(!absent.exists());
}
