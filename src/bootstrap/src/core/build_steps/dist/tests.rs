use std::path::{Path, PathBuf};

use super::reproducible_pgo_profiles;

#[test]
fn pgo_reproducible_profiles_preserve_collisions_and_deduplicate_shared_profiles() {
    let rustc = Some(PathBuf::from("rustc/default.profdata"));
    let rustdoc = Some(PathBuf::from("rustdoc/default.profdata"));
    let rustfmt = Some(PathBuf::from("rustfmt/rustfmt.profdata"));
    let omitted = None;

    let profiles = reproducible_pgo_profiles(
        [
            ("rustc", &rustc),
            ("rustdoc", &rustdoc),
            ("cargo", &rustc),
            ("clippy", &omitted),
            ("rustfmt", &rustfmt),
        ],
        &[],
    );

    assert_eq!(
        profiles,
        vec![
            (Path::new("rustc/default.profdata"), "."),
            (Path::new("rustdoc/default.profdata"), "rustdoc"),
            (Path::new("rustfmt/rustfmt.profdata"), "."),
        ]
    );
}

#[test]
fn pgo_reproducible_profiles_preserve_existing_reproducible_artifacts() {
    let rustc = Some(PathBuf::from("rustc/default.profdata"));
    let rustdoc = Some(PathBuf::from("rustdoc/default.profdata"));
    let rustfmt = Some(PathBuf::from("rustfmt/existing.profdata"));
    let reproducible_artifacts =
        vec!["bolt/default.profdata".to_string(), "rustfmt/existing.profdata".to_string()];

    let profiles = reproducible_pgo_profiles(
        [("rustc", &rustc), ("rustdoc", &rustdoc), ("rustfmt", &rustfmt)],
        &reproducible_artifacts,
    );

    assert_eq!(
        profiles,
        vec![
            (Path::new("rustc/default.profdata"), "rustc"),
            (Path::new("rustdoc/default.profdata"), "rustdoc"),
        ]
    );
}
