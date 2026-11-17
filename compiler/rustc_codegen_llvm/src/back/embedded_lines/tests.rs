use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use super::{physical_files, publish, read_file};

#[test]
fn physical_source_requires_one_checksum_and_literal_identity() {
    let changed = BTreeMap::from([(PathBuf::from("/src/a.rs"), BTreeMap::new())]);
    let one = br#"[["/src","a.rs",1,"0112"]]"#;
    let duplicate = br#"[["/src","a.rs",1,"0112"],["/","/src/a.rs",1,"0112"]]"#;
    let other = br#"[["/src","a.rs",1,"0112"],["/src","a.rs",1,"ae71"]]"#;
    assert_eq!(physical_files(one, &changed).unwrap().len(), 1);
    assert!(physical_files(duplicate, &changed).is_none());
    assert!(physical_files(other, &changed).is_none());
    assert!(physical_files(b"[]", &changed).is_none());
}

#[test]
fn publication_keeps_prior_members_and_reader_unchanged() {
    use std::io::Read;
    let dir = rustc_fs_util::TempDirBuilder::new()
        .prefix("rustc-lines-test-")
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    let prior = dir.path().join("prior");
    let session = dir.path().join("session");
    fs::write(&prior, b"original").unwrap();
    fs::hard_link(&prior, &session).unwrap();
    let mut open = fs::File::open(&session).unwrap();
    publish(&session, b"updated").unwrap();
    let mut old = vec![];
    open.read_to_end(&mut old).unwrap();
    assert_eq!(old, b"original");
    assert_eq!(read_file(&prior).unwrap(), b"original");
    assert_eq!(read_file(&session).unwrap(), b"updated");
    assert!(publish(&session.join("impossible"), b"invalid").is_none());
    assert_eq!(read_file(&session).unwrap(), b"updated");
}
