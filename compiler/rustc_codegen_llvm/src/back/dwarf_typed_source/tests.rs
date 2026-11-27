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
        path: Some(path.clone()),
        hash: SourceFileHash::new_in_memory(SourceFileHashAlgorithm::Md5, bytes),
    };
    let mut keys = Vec::new();
    append(&mut keys, b"/virtual").unwrap();
    append(&mut keys, b"input.rs").unwrap();
    fs::write(&path, previous).unwrap();
    let saved = capture(&keys, &[id(previous)]).unwrap();
    fs::write(&path, edited).unwrap();
    assert!(mapping(&saved, &[id(edited)], None).is_some());
    fs::write(&path, b"first\npoison\nlast\n").unwrap();
    assert!(mapping(&saved, &[id(edited)], None).is_none());
    assert!(capture(&keys, &[id(edited)]).is_none());
}

fn imported(name: &str, bytes: &[u8]) -> DwarfSourceFile {
    DwarfSourceFile {
        directory: "/virtual".into(),
        filename: name.into(),
        path: None,
        hash: SourceFileHash::new_in_memory(SourceFileHashAlgorithm::Md5, bytes),
    }
}

fn imported_certificate(files: &[DwarfSourceFile], lines: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    append(&mut out, b"/virtual").unwrap();
    // The moving runtime literal changes the normalized module digest.
    out.extend_from_slice(&[lines[0] as u8; 32]);
    out.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for file in files {
        append(&mut out, format!("/virtual/{}", file.filename).as_bytes()).unwrap();
        append(&mut out, hex(file.hash.hash_bytes()).as_bytes()).unwrap();
    }
    out.extend_from_slice(&((files.len() * lines.len()) as u32).to_le_bytes());
    for id in 0..files.len() {
        for line in lines {
            out.extend_from_slice(&(id as u32).to_le_bytes());
            out.extend_from_slice(&line.to_le_bytes());
        }
    }
    out
}

fn capture_imports(files: &[DwarfSourceFile]) -> Vec<u8> {
    let mut keys = Vec::new();
    for file in files {
        append(&mut keys, file.directory.as_bytes()).unwrap();
        append(&mut keys, file.filename.as_bytes()).unwrap();
    }
    capture(&keys, files).unwrap()
}

#[test]
fn proposes_imported_rows_only_from_both_checked_certificates() {
    let old = [imported("dep/lib.rs", b"first\nlast\n")];
    let new = [imported("dep/lib.rs", b"first\ninserted\nlast\n")];
    let saved = capture_imports(&old);
    let before = imported_certificate(&old, &[2, 1]);
    let after = imported_certificate(&new, &[3, 1]);
    let pair = Some((before.as_slice(), after.as_slice()));
    let (map, proof) = mapping(&saved, &new, pair).unwrap();
    let map: Value = serde_json::from_slice(&map).unwrap();
    let proof: Value = serde_json::from_slice(&proof).unwrap();
    assert_eq!(map["files"][0]["lines"], json!([[1, 1], [2, 3]]));
    assert_eq!(proof["version"], 3);
    assert!(proof["files"][0]["old"].get("sha1").is_none());

    assert!(mapping(&saved, &new, None).is_none());
    assert!(mapping(&saved, &new, Some((&after, &before))).is_none());
    assert!(mapping(&saved, &[imported("dep/lib.rs", b"other")], pair).is_none());
    assert!(mapping(&saved, &[new[0].clone(), old[0].clone()], pair).is_none());
    let mut local = new[0].clone();
    local.path = Some("dep/lib.rs".into());
    assert!(mapping(&saved, &[local], pair).is_none());
    for bad in [[1, 3], [1, 1], [3, 0], [u32::MAX, 1]] {
        let bad = imported_certificate(&new, &bad);
        assert!(mapping(&saved, &new, Some((&before, &bad))).is_none());
    }
    assert!(mapping(&saved, &new, Some((&before, &after[..after.len() - 1]))).is_none());
}

#[test]
fn bounds_the_number_and_extent_of_imported_maps() {
    let old = [imported("a.rs", b"a\n"), imported("b.rs", b"b\n")];
    let new = [imported("a.rs", b"\na\n"), imported("b.rs", b"\nb\n")];
    let before = imported_certificate(&old, &[1]);
    let after = imported_certificate(&new, &[2]);
    let pair = Some((before.as_slice(), after.as_slice()));
    for file in &old {
        assert!(mapping(&capture_imports(std::slice::from_ref(file)), &new, pair).is_some());
    }
    assert!(mapping(&capture_imports(&old), &new, pair).is_none());

    let before = imported_certificate(&old[..1], &(1..=65_537).collect::<Vec<_>>());
    let after = imported_certificate(&new[..1], &(2..=65_538).collect::<Vec<_>>());
    assert!(mapping(&capture_imports(&old[..1]), &new, Some((&before, &after))).is_none());
}

#[test]
fn conflicting_suffix_uses_only_verified_compiler_rows() {
    let prior = b"first\nold\nrepeat\nremoved\nrepeat\nend\n";
    let now = b"first\nnew\nrepeat\nend\n";
    let old = [imported("input.rs", prior)];
    let new = [imported("input.rs", now)];
    let before = imported_certificate(&old, &[2, 3, 6]);
    let after = imported_certificate(&new, &[2, 3, 4]);
    let (_, _, mut rows, _) = source_rows(prior, now).unwrap();
    assert_eq!(rows, [[1, 1], [5, 3], [6, 4]]);
    middle_rows((&before, &after), &new[0], prior, now, &mut rows).unwrap();
    assert_eq!(rows, [[3, 3], [6, 4]]);
}

#[test]
fn supplements_multi_hunk_caller_moves_with_another_files_debug_rows() {
    let dir = rustc_fs_util::TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let path = dir.path().join("caller.rs");
    let prior = b"first\nold\nmiddle\ngone\nlast\n";
    let now = b"first\nchanged\nextra\nmiddle\nnew\nlast\n";
    let mut old = [imported("caller.rs", prior), imported("other.rs", b"a\nb\nc\n")];
    let mut new = [imported("caller.rs", now), imported("other.rs", b"a\nb\nx\nc\n")];
    old[0].path = Some(path.clone());
    new[0].path = Some(path.clone());
    new[1].path = Some(dir.path().join("other.rs"));
    let before = imported_certificate(&old, &[3]);
    let after = imported_certificate(&new, &[4]);
    std::fs::write(&path, prior).unwrap();
    let saved = capture_imports(&old[..1]);
    std::fs::write(path, now).unwrap();
    let proposed = |inventory: &[DwarfSourceFile]| {
        let (map, _) = mapping(&saved, inventory, Some((&before, &after))).unwrap();
        serde_json::from_slice::<Value>(&map).unwrap()
    };
    let map = proposed(&new);
    assert_eq!(map["files"][0]["lines"], json!([[1, 1], [3, 4], [5, 6]]));
    assert_eq!(map["files"][1]["filename"], "other.rs");
    assert_eq!(map["files"][1]["lines"], json!([[3, 4]]));
    new[1].hash = old[1].hash;
    assert_eq!(proposed(&new)["files"].as_array().unwrap().len(), 1);
}
