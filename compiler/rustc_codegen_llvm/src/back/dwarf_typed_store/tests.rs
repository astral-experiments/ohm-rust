use super::*;

fn packet(ordered: &[u8]) -> Packet {
    Packet {
        ordered: ordered.to_vec(),
        owners: b"[]".to_vec(),
        sources: b"source".to_vec(),
        context: [7; 32],
    }
}

#[test]
fn old_linked_generation_keeps_its_native_object() {
    let root = TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let old = root.path().join("old");
    let new = root.path().join("new");
    fs::create_dir(&old).unwrap();
    fs::create_dir(&new).unwrap();
    let first = Store::new(&old, [3; 16], "cgu", 44);
    first.publish(&packet(b"original ordered IR"), b"original object").unwrap();
    let second = Store::new(&new, [3; 16], "cgu", 44);
    fs::hard_link(&first.record, &second.record).unwrap();
    let mut reader = std::fs::File::open(&second.record).unwrap();
    let original_record = fs::read(&first.record).unwrap();
    fs::write(new.join("cgu.o"), b"intervening native").unwrap();
    assert_eq!(Store::donor(&second.read().unwrap()).unwrap().object, b"original object");
    assert!(!second.should_enroll(false));
    second.remember_miss();
    assert!(second.should_enroll(false));
    second.publish(&packet(b"replacement IR"), b"replacement object").unwrap();
    assert_eq!(Store::donor(&second.read().unwrap()).unwrap().object, b"replacement object");
    assert_eq!(Store::donor(&first.read().unwrap()).unwrap().object, b"original object");
    use std::io::Read;
    let mut retained = Vec::new();
    reader.read_to_end(&mut retained).unwrap();
    assert_eq!(retained, original_record);
    assert!(!second.should_enroll(false));
}

#[test]
fn corrupt_wrong_unit_or_shape_declines() {
    let root = TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let original = Store::new(root.path(), [3; 16], "cgu", 44);
    original.publish(&packet(b"IR"), b"object").unwrap();
    assert!(Store::new(root.path(), [4; 16], "cgu", 44).read().is_none());
    assert!(Store::new(root.path(), [3; 16], "cgu", 45).read().is_none());
    let wrong = Store::new(root.path(), [3; 16], "renamed", 44);
    fs::copy(&original.record, &wrong.record).unwrap();
    assert!(wrong.read().is_none());
    let mut corrupt = fs::read(&original.record).unwrap();
    *corrupt.last_mut().unwrap() ^= 1;
    fs::write(&original.record, corrupt).unwrap();
    assert!(original.read().is_none());
}

#[test]
fn failed_publication_preserves_real_output_and_pending_receipt() {
    let root = TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let work = root.path().join("cgu.o");
    fs::write(&work, b"ordinary object").unwrap();
    let store = Store::new(root.path(), [1; 16], "cgu", 9);
    store.remember_miss();
    fs::create_dir(&store.record).unwrap();
    assert!(store.publish(&packet(b"IR"), b"ordinary object").is_none());
    assert!(store.read().is_none());
    assert!(store.should_enroll(false));
    assert_eq!(fs::read(work).unwrap(), b"ordinary object");
}
