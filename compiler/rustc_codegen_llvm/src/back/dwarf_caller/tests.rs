use rustc_codegen_ssa::back::write::DwarfSourceFile;
use rustc_span::{SourceFileHash, SourceFileHashAlgorithm};

use super::super::dwarf_typed_source;
use super::*;

fn counted(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend(u32::try_from(bytes.len()).unwrap().to_le_bytes());
    out.extend(bytes);
}

fn certificate(bytes: &[u8], coordinate: u32, caller: u32, semantic: u8) -> Vec<u8> {
    let mut e3 = Vec::new();
    counted(&mut e3, b"/virtual");
    e3.extend([semantic; 32]);
    e3.extend(1u32.to_le_bytes());
    counted(&mut e3, b"file.rs");
    let hash = SourceFileHash::new_in_memory(SourceFileHashAlgorithm::Md5, bytes);
    let hash: String = hash.hash_bytes().iter().map(|b| format!("{b:02x}")).collect();
    counted(&mut e3, hash.as_bytes());
    e3.extend(1u32.to_le_bytes());
    e3.extend(0u32.to_le_bytes());
    e3.extend(coordinate.to_le_bytes());
    let mut packet = MAGIC.to_vec();
    let coordinates = serde_json::to_vec(&[caller]).unwrap();
    for field in [&e3[..], br#"[[2,4,"","","/virtual","file.rs",true,7]]"#, &coordinates] {
        packet.extend(u64::try_from(field.len()).unwrap().to_le_bytes());
        packet.extend(field);
    }
    packet
}

#[test]
fn source_bound_caller_move() {
    let dir = rustc_fs_util::TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let path = dir.path().join("file.rs");
    let old = b"begin\nlocation\nend\n";
    let new = b"begin\ninsert\nlocation\nend\n";
    let file = |bytes: &[u8]| DwarfSourceFile {
        directory: "/virtual".into(),
        filename: "file.rs".into(),
        path: path.clone(),
        hash: SourceFileHash::new_in_memory(SourceFileHashAlgorithm::Md5, bytes),
    };
    let mut keys = Vec::new();
    counted(&mut keys, b"/virtual");
    counted(&mut keys, b"file.rs");
    std::fs::write(&path, old).unwrap();
    let old_source = dwarf_typed_source::capture(&keys, &[file(old)]).unwrap();
    std::fs::write(&path, new).unwrap();
    let mapping = dwarf_typed_source::mapping(&old_source, &[file(new)]).unwrap();
    let previous = certificate(old, 2, 2, 4);
    let current = certificate(new, 3, 3, 4);
    let proposed: Vec<Value> =
        serde_json::from_slice(&changes(&previous, &current, &mapping).unwrap()).unwrap();
    assert_eq!(proposed[0]["old_line"], 2);
    assert_eq!(proposed[0]["new_line"], 3);
    assert_eq!(proposed[0]["column"], 7);
    assert!(
        dwarf_certificate::compare(&ordinary(&previous).unwrap(), &ordinary(&current).unwrap())
            .is_none()
    );
    assert!(changes(&previous, &certificate(new, 3, 4, 4), &mapping).is_none());
    assert!(changes(&previous, &certificate(new, 4, 3, 4), &mapping).is_none());
    assert!(changes(&previous, &certificate(new, 3, 3, 5), &mapping).is_none());
    assert!(changes(&previous, &certificate(b"different contents", 3, 3, 4), &mapping).is_none());
}

#[test]
fn truncated_or_appended_projection() {
    let data = certificate(b"x", 1, 1, 2);
    assert!(ordinary(&data[1..]).is_none());
    assert!(ordinary(&data[..data.len() - 1]).is_none());
    let mut appended = data;
    appended.push(0);
    assert!(ordinary(&appended).is_none());
}
