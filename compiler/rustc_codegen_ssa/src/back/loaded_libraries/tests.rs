use super::*;

fn maps_line(path: &Path, id: FileId) -> Vec<u8> {
    let (major, minor, inode) = id;
    let mut maps = format!("20-30  r-xp  0\t{major:x}:{minor:x} {inode}   ").into_bytes();
    maps.extend(path.as_os_str().as_bytes());
    maps.push(b'\n');
    maps
}

#[test]
fn uses_the_real_backend_file_and_rejects_replacements() {
    let dir = tempfile::tempdir().unwrap();
    let llvm = dir.path().join(OsStr::from_bytes(b"external LLVM\xff.so"));
    fs::write(&llvm, b"backend").unwrap();
    let mapped = mapped_file_id(&File::open(&llvm).unwrap()).unwrap();
    let maps = maps_line(&llvm, mapped);
    let initial = provider(&maps, 0x2f).unwrap();
    assert_eq!(initial.path, llvm);
    assert!(provider(&maps, 0x30).is_none());
    let mut deleted = maps[..maps.len() - 1].to_vec();
    deleted.extend(b" (deleted)\n");
    assert!(provider(&deleted, 0x2f).is_none());

    fs::write(&llvm, b"updated backend").unwrap();
    let updated = provider(&maps, 0x2f).unwrap();
    assert_eq!(updated.mapping, initial.mapping);
    assert_ne!(updated, initial);

    // Keep the first inode alive while giving its pathname to identical bytes.
    fs::rename(&llvm, dir.path().join("old")).unwrap();
    fs::write(&llvm, b"updated backend").unwrap();
    assert!(provider(&maps, 0x2f).is_none());
    let replacement = mapped_file_id(&File::open(&llvm).unwrap()).unwrap();
    assert_ne!(replacement, mapped);
    assert_ne!(provider(&maps_line(&llvm, replacement), 0x2f).unwrap(), updated);
    fs::remove_file(&llvm).unwrap();
    assert!(provider(&maps, 0x2f).is_none());
}
