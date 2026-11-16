use super::*;

#[test]
fn repeated_rows_are_ambiguous() {
    let (_, _, safe, ambiguous) = insertion(b"a\nb\nb\nc\n", b"a\nb\nb\nb\nc\n").unwrap();
    assert_eq!(safe, [[1, 1], [4, 5]]);
    assert_eq!(ambiguous, [2, 3]);
    assert!(insertion(b"a\nb\n", b"a\nc\n").is_none());
    assert!(insertion(b"a\nb\n", b"c\nx\nb\n").is_none());
}

#[test]
fn replaces_only_between_disjoint_unchanged_boundaries() {
    let (_, _, rows, ambiguous) =
        source_rows(b"prefix\nold1\nold2\nlast\n", b"prefix\nnew\nlast\n").unwrap();
    assert_eq!(rows, [[1, 1], [4, 3]]);
    assert!(ambiguous.is_empty());
    assert!(source_rows(b"a\nb\nb\nb\nc\n", b"a\nb\nb\nc\n").is_none());
    assert!(source_rows(b"a\nz\n", b"a\nc\nd\n").is_none());
}

#[test]
fn later_filesystem_bytes_must_match_the_frontend_hash() {
    use std::fs;
    let root = rustc_fs_util::TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let path = root.path().join("input.rs");
    let previous = b"first\nlast\n";
    let edited = b"first\ninsert\nlast\n";
    let id = |bytes: &[u8]| DwarfSourceFile {
        directory: "/virtual".into(),
        filename: "input.rs".into(),
        path: path.clone(),
        hash: SourceFileHash::new_in_memory(SourceFileHashAlgorithm::Md5, bytes),
    };
    let mut keys = Vec::new();
    append(&mut keys, b"/virtual").unwrap();
    append(&mut keys, b"input.rs").unwrap();
    fs::write(&path, previous).unwrap();
    let saved = capture(&keys, &[id(previous)]).unwrap();
    fs::write(&path, edited).unwrap();
    assert!(mapping(&saved, &[id(edited)]).is_some());
    fs::write(&path, b"first\npoison\nlast\n").unwrap();
    assert!(mapping(&saved, &[id(edited)]).is_none());
    assert!(capture(&keys, &[id(edited)]).is_none());
}
