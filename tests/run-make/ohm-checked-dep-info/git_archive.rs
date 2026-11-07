use std::path::Path;

use run_make_support::{
    build_root, cmd, has_extension, has_prefix, path, python_command, recursive_find_files, rfs,
    rustc_path, shallow_find_files,
};

pub const GUARD: &str = "/dev/null/ohm-checked-bundled-archives-v1";

pub fn assert_proof(dep_info: &str, archive: &Path) {
    let mut lines = dep_info.lines();
    assert_eq!(lines.next(), Some("# ohm-raw-checksum-paths-v2"));
    assert_eq!(lines.next(), Some("# ohm-bundled-archives-v1:1"));
    assert_eq!(lines.next().unwrap(), format!("# ohm-bundled-archive-v1:{}", archive.display()));
    assert!(dep_info.lines().any(|line| line == format!("{GUARD}:")), "{dep_info}");
}

pub fn retarget() {
    let git = |args: &[&str]| {
        cmd("git")
            .current_dir("git-provider")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .run();
    };
    git(&["init", "-q"]);
    git(&["add", "-f", "Cargo.toml", "build.rs", "src/lib.rs", "v7.a", "v9.a"]);
    git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-qm",
        "fixture",
    ]);
    let url = python_command()
        .args(["-c", "from pathlib import Path; print(Path('git-provider').resolve().as_uri())"])
        .run()
        .stdout_utf8();
    let manifest = rfs::read_to_string("git-app/Cargo.toml.in").replace("$GIT_URL", url.trim());
    rfs::write("git-app/Cargo.toml", manifest);

    // Run Cargo from the bootstrap toolchain against the compiler under test.
    let build = || {
        cmd(build_root().join("stage0/bin/cargo"))
            .args(["-Zchecksum-freshness", "build", "--manifest-path", "git-app/Cargo.toml"])
            .env("RUSTC", rustc_path())
            .env("RUSTC_BOOTSTRAP", "1")
            .env("CARGO_ENCODED_RUSTFLAGS", "-Zohm-check-relink-inputs")
            .env("CARGO_HOME", path("cargo-home"))
            .env("CARGO_TARGET_DIR", path("cargo-target"))
            .env_remove("CARGO_BUILD_BUILD_DIR")
            .run();
    };
    build();
    cmd(path("cargo-target/debug/archive_app")).run().assert_stdout_equals("7");
    let [archive]: [_; 1] =
        recursive_find_files(path("cargo-target"), |p| p.ends_with("libprobe.a"))
            .try_into()
            .unwrap();
    let [depfile]: [_; 1] = shallow_find_files(path("cargo-target/debug/deps"), |p| {
        has_prefix(p, "archive_provider-") && has_extension(p, "d")
    })
    .try_into()
    .unwrap();
    assert_proof(&rfs::read_to_string(depfile), &archive);
    let first = std::fs::read_link(&archive).unwrap();
    assert!(first.starts_with(path("cargo-home/git/checkouts")) && first.ends_with("v7.a"));
    rfs::remove_file(&archive);
    rfs::symlink_file(first.with_file_name("v9.a"), &archive);
    build();
    cmd(path("cargo-target/debug/archive_app")).run().assert_stdout_equals("9");
}
