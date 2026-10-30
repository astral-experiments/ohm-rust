use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

use rustc_fs_util::TempDirBuilder;

use super::*;

#[cfg(target_os = "linux")]
fn object(value: u8) -> Vec<u8> {
    let mut bytes = vec![value; 32 * 1024];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    bytes[16..18].copy_from_slice(&1u16.to_le_bytes());
    bytes
}

#[cfg(target_os = "linux")]
#[test]
fn packs_only_private_and_superseded_objects_and_restores_exact_bytes() {
    let tmp = TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let dir = tmp.path().join("s-2222-_000000000000000a_ab-working");
    let previous = tmp.path().join("s-1111-_000000000000000a_cd-svh");
    let target = tmp.path().join("target");
    for path in [&dir, &previous, &target] {
        fs::create_dir(path).unwrap();
    }
    let bytes = object(17);
    for name in ["a.o", "b.o", "c.o", "raw.rlink"] {
        fs::write(dir.join(name), &bytes).unwrap();
    }
    fs::hard_link(dir.join("b.o"), previous.join("b.o")).unwrap();
    fs::hard_link(dir.join("c.o"), tmp.path().join("exported.o")).unwrap();
    symlink(tmp.path().join("exported.o"), dir.join("link.o")).unwrap();
    let original = fs::metadata(dir.join("a.o")).unwrap();
    objects::pack(&dir, &previous);
    for name in ["a.o", "b.o"] {
        assert!(!dir.join(name).exists(), "{name}");
    }
    assert_eq!(fs::read(previous.join("b.o")).unwrap(), bytes);
    assert!(fs::symlink_metadata(dir.join("link.o")).unwrap().is_symlink());
    assert_eq!(
        fs::metadata(dir.join("c.o")).unwrap().ino(),
        fs::metadata(tmp.path().join("exported.o")).unwrap().ino()
    );
    assert_eq!(fs::read(dir.join("raw.rlink")).unwrap(), bytes);
    let published = dir.with_file_name("s-2222-_000000000000000a_ab-svh");
    fs::rename(dir, &published).unwrap();
    for entry in fs::read_dir(&published).unwrap() {
        let name = entry.unwrap().file_name();
        objects::restore(&published, &target, &name).unwrap();
    }
    assert_eq!(fs::read(target.join("a.o")).unwrap(), bytes);
    assert_eq!(fs::read(target.join("b.o")).unwrap(), bytes);
    let restored = fs::metadata(target.join("a.o")).unwrap();
    assert_eq!(restored.permissions().mode(), original.permissions().mode());
    assert_eq!(restored.modified().unwrap(), original.modified().unwrap());
}

#[cfg(target_os = "linux")]
#[test]
fn rejects_swapped_object_contents_with_the_same_session_and_filename() {
    let tmp = TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let make = |root: &str, value| {
        let dir = tmp.path().join(root).join("s-2222-_000000000000000a_ab-working");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.o"), object(value)).unwrap();
        objects::pack(&dir, tmp.path());
        let saved = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().file_name();
        assert!(saved.to_str().unwrap().starts_with("ohm-packed-object-"));
        (dir, saved)
    };
    let (dir, saved) = make("first", 17);
    let (other, alternate) = make("second", 29);
    let original = fs::read(dir.join(&saved)).unwrap();
    for (i, bad) in [
        fs::read(other.join(alternate)).unwrap(),
        original[..original.len() - 1].to_vec(),
        [original.as_slice(), b"extra"].concat(),
    ]
    .into_iter()
    .enumerate()
    {
        let dst = tmp.path().join(i.to_string());
        fs::create_dir(&dst).unwrap();
        fs::write(dir.join(&saved), bad).unwrap();
        assert!(objects::restore(&dir, &dst, &saved).is_err());
    }
    // A complete raw file can coexist with a stream after an interrupted unlink.
    fs::write(dir.join("a.o"), object(17)).unwrap();
    let dst = tmp.path().join("interrupted");
    fs::create_dir(&dst).unwrap();
    assert!(objects::restore(&dir, &dst, &saved).unwrap());
    assert!(!dst.join("a.o").exists());
}

fn fixture(dir: &Path) -> Vec<Vec<u8>> {
    TABLES
        .iter()
        .map(|t| {
            let mut bytes = vec![t.tag; MIN_SIZE as usize * 2];
            bytes[..4].copy_from_slice(b"RSIC");
            fs::write(dir.join(t.names[0]), &bytes).unwrap();
            fs::hard_link(dir.join(t.names[0]), dir.join(t.names[1])).unwrap();
            bytes
        })
        .collect()
}

#[test]
fn hydrates_only_private_tables_and_their_inode_aliases() {
    let tmp = TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let dir = tmp.path();
    let raw = fixture(dir);
    fs::write(dir.join(CHECK_DEP_GRAPH), b"independent stage").unwrap();
    let before: Vec<_> =
        TABLES.iter().map(|t| fs::metadata(dir.join(t.names[0])).unwrap()).collect();
    pack(dir);
    for t in &TABLES {
        assert!(dir.join(t.packed).is_file());
        assert!(!dir.join(t.names[0]).exists());
        assert!(!dir.join(t.names[1]).exists());
    }
    assert_eq!(fs::read(dir.join(CHECK_DEP_GRAPH)).unwrap(), b"independent stage");
    for case in ["first", "concurrent"] {
        let target = dir.join(case);
        fs::create_dir(&target).unwrap();
        restore(&TABLES.iter().collect::<Vec<_>>(), dir, &target).unwrap();
        for (i, t) in TABLES.iter().enumerate() {
            assert_eq!(fs::read(target.join(t.names[0])).unwrap(), raw[i]);
            let m = fs::metadata(target.join(t.names[0])).unwrap();
            assert_eq!(m.ino(), fs::metadata(target.join(t.names[1])).unwrap().ino());
            assert_eq!(m.permissions().mode(), before[i].permissions().mode());
            assert_eq!(m.modified().unwrap(), before[i].modified().unwrap());
            assert!(!target.join(t.names[2]).exists());
        }
    }
    let name = DEP_GRAPH_FILENAME;
    assert_ne!(
        fs::metadata(dir.join("first").join(name)).unwrap().ino(),
        fs::metadata(dir.join("concurrent").join(name)).unwrap().ino()
    );
}

#[test]
fn externally_shared_or_missing_group_leaves_both_tables_raw() {
    let tmp = TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let dir = tmp.path();
    let raw = fixture(dir);
    fs::hard_link(dir.join(QUERY_CACHE_FILENAME), dir.join("other-session")).unwrap();
    pack(dir);
    assert_eq!(fs::read(dir.join(DEP_GRAPH_FILENAME)).unwrap(), raw[0]);
    assert_eq!(fs::read(dir.join(QUERY_CACHE_FILENAME)).unwrap(), raw[1]);
    assert!(TABLES.iter().all(|t| !dir.join(t.packed).exists()));
    fs::remove_file(dir.join(QUERY_CACHE_FILENAME)).unwrap();
    pack(dir);
    assert!(TABLES.iter().all(|t| !dir.join(t.packed).exists()));
    assert!(restore(&[], dir, dir).is_ok());
}

#[test]
fn rejects_incomplete_or_mixed_generations_and_damaged_streams() {
    let tmp = TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let source = tmp.path();
    fixture(source);
    pack(source);
    let tables = [&TABLES[0], &TABLES[1]];
    let graph = source.join(TABLES[0].packed);
    let valid = fs::read(&graph).unwrap();
    let mut bad_checksum = valid.clone();
    *bad_checksum.last_mut().unwrap() ^= 1;
    let mut extra = valid.clone();
    extra.push(0);
    let wrong_table = fs::read(source.join(TABLES[1].packed)).unwrap();
    let mut oversized = ZlibEncoder::new(Vec::new(), Compression::fast());
    oversized.write_all(&[1, b'G', 1]).unwrap();
    oversized.write_all(&(LIMIT + 1).to_le_bytes()).unwrap();
    let mut bad_magic = ZlibEncoder::new(Vec::new(), Compression::fast());
    bad_magic.write_all(&[1, b'G', 1]).unwrap();
    bad_magic.write_all(&MIN_SIZE.to_le_bytes()).unwrap();
    bad_magic.write_all(session_prefix(source).unwrap()).unwrap();
    bad_magic.write_all(&[0]).unwrap();
    bad_magic.write_all(&vec![0; MIN_SIZE as usize]).unwrap();
    for (i, bad) in [
        bad_checksum,
        valid[..valid.len() - 1].to_vec(),
        extra,
        wrong_table,
        oversized.finish().unwrap(),
        bad_magic.finish().unwrap(),
    ]
    .into_iter()
    .enumerate()
    {
        let dst = source.join(format!("bad-{i}"));
        fs::create_dir(&dst).unwrap();
        fs::write(&graph, bad).unwrap();
        assert!(restore(&tables, source, &dst).is_err());
    }
    fs::write(&graph, &valid).unwrap();
    let foreign = source.join("s-1234-another-working");
    fs::create_dir(&foreign).unwrap();
    fixture(&foreign);
    pack(&foreign);
    fs::copy(foreign.join(TABLES[0].packed), &graph).unwrap();
    let dst = source.join("foreign");
    fs::create_dir(&dst).unwrap();
    assert!(restore(&tables, source, &dst).is_err());
    fs::write(&graph, &valid).unwrap();
    let dst = source.join("mixed");
    fs::create_dir(&dst).unwrap();
    assert!(restore(&tables[..1], source, &dst).is_err());
    fs::write(dst.join(DEP_GRAPH_FILENAME), b"old compiler's raw table").unwrap();
    assert!(restore(&tables, source, &dst).is_err());
    assert_eq!(fs::read(dst.join(DEP_GRAPH_FILENAME)).unwrap(), b"old compiler's raw table");
    fs::remove_file(&graph).unwrap();
    symlink(source.join(TABLES[1].packed), &graph).unwrap();
    assert!(restore(&tables, source, &dst).is_err());
}
