//@ ignore-cross-compile

use std::path::PathBuf;

use run_make_support::{recursive_find_files, rfs, run, rustc};

fn compile(incremental: &str, emit: &str, state: &str, shared_stages: bool) {
    let mut command = rustc();
    command
        .input("main.rs")
        .incremental(incremental)
        .emit(emit)
        .arg(format!("-Zassert-incr-state={state}"))
        .arg("-Zincremental-verify-ich");
    if shared_stages {
        command.arg("-Zohm-shared-stages");
    }
    command.run().assert_stderr_contains("unused variable: `unused`");
    if emit == "link" {
        run("main").assert_stdout_equals("42\n");
    }
}

fn caches(incremental: &str, filename: &str) -> Vec<PathBuf> {
    let paths = recursive_find_files(incremental, |path| path.file_name().unwrap() == filename);
    assert!(!paths.is_empty(), "missing {filename} in {incremental}");
    paths
}

fn main() {
    for damage in ["missing", "header", "short-header", "trailer"] {
        compile(damage, "link", "not-loaded", false);
        compile(damage, "link", "loaded", false);
        for path in caches(damage, "query-cache.bin") {
            let mut bytes = rfs::read(&path);
            // Replace the file instead of modifying any hardlinked snapshots.
            rfs::remove_file(&path);
            match damage {
                "missing" => continue,
                "header" => bytes[0] ^= 1,
                "short-header" => bytes.truncate(2),
                "trailer" => bytes.truncate(bytes.len() - 1),
                _ => unreachable!(),
            }
            rfs::write(path, bytes);
        }
        compile(damage, "link", "not-loaded", false);
        compile(damage, "link", "loaded", false);
    }

    compile("retained", "link", "not-loaded", false);
    let seed = rfs::read(&caches("retained", "query-cache.bin")[0]);
    compile("retained", "link", "loaded", false);
    let mut damaged = false;
    for path in caches("retained", "query-cache.bin") {
        let mut bytes = rfs::read(&path);
        if bytes.len() > seed.len() && bytes.starts_with(&seed) {
            bytes[seed.len() - 1] ^= 1;
            rfs::remove_file(&path);
            rfs::write(path, bytes);
            damaged = true;
        }
    }
    assert!(damaged, "no retained generation found");
    compile("retained", "link", "not-loaded", false);
    compile("retained", "link", "loaded", false);

    // An incompatible graph must be rejected before decoding its query cache.
    compile("options", "link", "not-loaded", false);
    for path in caches("options", "query-cache.bin") {
        let mut bytes = rfs::read(&path);
        let trailer = b"rust-end-file";
        assert!(bytes.ends_with(trailer));
        let footer = bytes.len() - trailer.len() - 8;
        bytes[footer..footer + 8].fill(u8::MAX);
        rfs::remove_file(&path);
        rfs::write(path, bytes);
    }
    for state in ["not-loaded", "loaded"] {
        rustc()
            .input("main.rs")
            .incremental("options")
            .opt_level("1")
            .arg(format!("-Zassert-incr-state={state}"))
            .arg("-Zincremental-verify-ich")
            .run()
            .assert_stderr_contains("unused variable: `unused`");
        run("main").assert_stdout_equals("42\n");
    }

    for missing in ["ordinary", "checkpoint", "both"] {
        compile(missing, "link", "not-loaded", true);
        compile(missing, "metadata", "loaded", true);
        if missing != "checkpoint" {
            for path in caches(missing, "query-cache.bin") {
                rfs::remove_file(path);
            }
        }
        if missing != "ordinary" {
            for path in caches(missing, "ohm-build-query-cache.bin") {
                rfs::remove_file(path);
            }
        }
        let state = if missing == "both" { "not-loaded" } else { "loaded" };
        compile(missing, "link", state, true);
        compile(missing, "link", "loaded", true);
    }
}
