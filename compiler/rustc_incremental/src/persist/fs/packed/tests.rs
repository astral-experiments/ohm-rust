use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

use rustc_fs_util::TempDirBuilder;

use super::*;

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
