use super::*;

#[test]
fn verifies_bundled_and_external_llvm_against_the_live_inode() {
    let dir = tempfile::tempdir().unwrap();
    let driver = dir.path().join("driver");
    let llvm = dir.path().join(OsStr::from_bytes(b"external LLVM\xff.so"));
    fs::write(&driver, b"compiler").unwrap();
    fs::write(&llvm, b"backend").unwrap();
    let driver_id = file_id(&driver.metadata().unwrap());
    let llvm_id = file_id(&llvm.metadata().unwrap());
    let mut maps = Vec::new();
    for (range, path, (major, minor, inode)) in
        [("10-20", &driver, driver_id), ("20-30", &llvm, llvm_id)]
    {
        maps.extend(format!("{range} r-xp 0 {major:x}:{minor:x} {inode}    ").bytes());
        maps.extend(path.as_os_str().as_bytes());
        maps.push(b'\n');
    }
    let addresses = [0x10, 0x13, 0x2f];
    let bundled = providers(&maps, addresses, &[driver_id, llvm_id], true).unwrap();
    assert_eq!(providers(&maps, addresses, &[driver_id], false).unwrap(), bundled);
    assert_eq!(bundled[2].path, llvm);
    assert!(providers(&maps, addresses, &[driver_id], true).is_none());
    assert!(providers(&maps, [0x10, 0x13, 0x40], &[driver_id], false).is_none());
    let mut deleted = maps[..maps.len() - 1].to_vec();
    deleted.extend(b" (deleted)\n");
    assert!(providers(&deleted, addresses, &[driver_id], false).is_none());
    fs::write(&llvm, b"updated backend").unwrap();
    let changed = providers(&maps, addresses, &[driver_id], false).unwrap();
    assert_eq!(changed[2].id, llvm_id);
    assert_ne!(changed, bundled);
    // Keep the mapped inode alive while replacing its filename with equal bytes.
    fs::rename(&llvm, dir.path().join("old")).unwrap();
    fs::write(&llvm, b"updated backend").unwrap();
    assert_ne!(file_id(&llvm.metadata().unwrap()), llvm_id);
    assert!(providers(&maps, addresses, &[driver_id], false).is_none());
    fs::remove_file(&llvm).unwrap();
    assert!(providers(&maps, addresses, &[driver_id], false).is_none());
    assert!(providers(&maps, [0x10, 0x13, 0x10], &[driver_id], false).is_some());
}
