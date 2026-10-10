use super::*;

#[test]
fn validate_bytes_read_and_negative_inputs() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("edit.rs");
    let other = directory.path().join("other.rs");
    fs::write(&source, "one").unwrap();
    fs::write(&other, "two").unwrap();
    let mut inputs = Inputs::default();
    inputs.record(&source, Input::Contents(hash_file(&source).unwrap()));
    inputs.record(&other, Input::Contents(hash_file(&other).unwrap()));
    assert!(inputs.unchanged(None));
    fs::write(&source, "new").unwrap();
    assert!(!inputs.unchanged(None));
    assert!(inputs.unchanged(Some(&fs::canonicalize(&source).unwrap())));
    fs::write(&other, "new").unwrap();
    assert!(!inputs.unchanged(Some(&fs::canonicalize(&source).unwrap())));

    let missing = directory.path().join("missing.rs");
    let mut inputs = Inputs::default();
    inputs.record(&missing, Input::Exists(false));
    assert!(inputs.unchanged(None));
    fs::write(&missing, "new module").unwrap();
    assert!(!inputs.unchanged(None));
}

#[test]
fn reject_inputs_that_changed_while_being_read() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("edit.rs");
    fs::write(&source, "old").unwrap();
    let mut inputs = Inputs::default();
    inputs.record(&source, Input::Contents(hash_file(&source).unwrap()));
    fs::write(&source, "new").unwrap();
    inputs.record(&source, Input::Contents(hash_file(&source).unwrap()));
    assert!(!inputs.unchanged(None));
}

#[cfg(unix)]
#[test]
fn redirecting_another_input_to_the_edited_file_does_not_exempt_it() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("edit.rs");
    let other = directory.path().join("other.rs");
    fs::write(&source, "new").unwrap();
    fs::write(&other, "old").unwrap();
    let mut inputs = Inputs::default();
    inputs.record(&other, Input::Contents(hash_file(&other).unwrap()));
    fs::remove_file(&other).unwrap();
    std::os::unix::fs::symlink(&source, &other).unwrap();
    assert!(!inputs.unchanged(Some(&fs::canonicalize(&source).unwrap())));
}
