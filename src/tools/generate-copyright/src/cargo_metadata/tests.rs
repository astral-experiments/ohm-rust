use super::*;

#[test]
fn external_path_dependency_notices_are_included() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("rust");
    let dependency = temporary.path().join("external");
    let vendor = temporary.path().join("vendor");
    for directory in [&root, &dependency, &vendor] {
        std::fs::create_dir_all(directory).unwrap();
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'compiler'\nversion = '0.1.0'\n\
         [workspace]\n[lib]\npath = 'lib.rs'\n\
         [dependencies]\nexternal = { path = '../external' }\n",
    )
    .unwrap();
    std::fs::write(
        dependency.join("Cargo.toml"),
        "[package]\nname = 'external'\nversion = '0.1.0'\n\
         license = 'MIT OR Apache-2.0'\n[lib]\npath = 'lib.rs'\n",
    )
    .unwrap();
    std::fs::write(root.join("lib.rs"), "").unwrap();
    std::fs::write(dependency.join("lib.rs"), "").unwrap();
    let notice = "Copyright Example contributors\nPermission is hereby granted...\n";
    std::fs::write(dependency.join("LICENSE-MIT"), notice).unwrap();
    let packages = get_metadata_and_notices(
        Path::new(env!("CARGO")),
        &temporary.path().join("cargo-home"),
        &vendor,
        &root,
        &[root.join("Cargo.toml")],
    )
    .unwrap();
    assert_eq!(packages.len(), 1, "in-tree packages are covered by reuse");
    let dependency = &packages[&Package { name: "external".into(), version: "0.1.0".into() }];
    assert_eq!(dependency.license, "MIT OR Apache-2.0");
    assert_eq!(dependency.notices["LICENSE-MIT"], notice);
}
