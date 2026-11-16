use super::*;

#[test]
fn embedded_receipt_binds_the_input_and_donor_object() {
    let dir = TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let mut entry = Entry {
        name: "worker".into(),
        configuration: Fingerprint::ZERO,
        exact: Some([19; 32]),
        proof: Vec::new(),
        optimized: None,
        previous: None,
        object: dir.path().join("current.o"),
        donor_object: dir.path().join("previous.o"),
        certificate: dir.path().join("worker.dwarf-lines"),
        split: None,
        register_comment: false,
        roster: None,
    };
    let module = CompiledModule {
        name: entry.name.clone(),
        kind: ModuleKind::Regular,
        object: Some(entry.object.clone()),
        dwarf_object: None,
        global_asm_object: None,
        bytecode: None,
        assembly: None,
        llvm_ir: None,
        links_from_incr_cache: Vec::new(),
    };
    fs::write(&entry.object, b"previous object").unwrap();
    entry.try_store(&module).unwrap();
    fs::rename(&entry.object, &entry.donor_object).unwrap();
    assert!(read_certificate(&entry.certificate, MAGIC, entry.configuration).is_none());
    assert!(
        read_certificate(&entry.certificate, EMBEDDED_MAGIC, Fingerprint::from_le_bytes([1; 16]))
            .is_none()
    );
    entry.previous = read_certificate(&entry.certificate, EMBEDDED_MAGIC, entry.configuration);
    entry.load().unwrap();
    assert!(entry.load_optimized().is_none());
    let mut truncated = entry.previous.clone().unwrap();
    truncated[120..124].copy_from_slice(&1_u32.to_le_bytes());
    assert!(proofs(&truncated).is_none());
    assert_eq!(fs::read(&entry.object).unwrap(), b"previous object");
    entry.exact = Some([20; 32]);
    assert!(entry.load().is_none());
    entry.exact = Some([19; 32]);
    fs::write(&entry.donor_object, b"replaced object").unwrap();
    assert!(entry.load().is_none());
    let mut bytes = read_file(&entry.certificate).unwrap();
    let other = Fingerprint::from_le_bytes([1; 16]);
    bytes[8..24].copy_from_slice(&other.to_le_bytes());
    fs::write(&entry.certificate, &bytes).unwrap();
    assert!(read_certificate(&entry.certificate, EMBEDDED_MAGIC, other).is_none());
    bytes[8..24].copy_from_slice(&entry.configuration.to_le_bytes());
    bytes[88] ^= 1;
    fs::write(&entry.certificate, bytes).unwrap();
    assert!(read_certificate(&entry.certificate, EMBEDDED_MAGIC, entry.configuration).is_none());
}

#[test]
fn changed_roster_declines_one_generation_and_preserves_the_old_session() {
    let dir = TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let old = dir.path().join("old.dwarf-lines");
    let new = dir.path().join("new.dwarf-lines");
    let mut receipt = vec![0; 124];
    receipt[..8].copy_from_slice(NORMALIZED_ROSTER_MAGIC);
    receipt.extend(9u64.to_le_bytes());
    fs::write(&old, &receipt).unwrap();
    fs::hard_link(&old, &new).unwrap();
    assert!(!decline_changed_roster(&new, Some(&receipt), Some(9)));
    assert!(decline_changed_roster(&new, Some(&receipt), Some(10)));
    assert!(!new.exists());
    assert_eq!(fs::read(&old).unwrap(), receipt);
    assert!(!decline_changed_roster(&new, None, Some(10)));
    assert!(!decline_changed_roster(&new, Some(&receipt), Some(10)));
    assert_eq!(proofs(&receipt), Some((&[][..], &[][..])));
    assert!(!has_exact_key(&receipt));
}
