//@ only-linux
//@ ignore-cross-compile
//@ needs-dynamic-linking

//! Exercise failures at the actual query-cache write and session rename, without compiler
//! failpoints or a full filesystem. The interposer is confined to this test's rustc children.

use std::fs;
use std::path::{Path, PathBuf};

use run_make_support::{cc, rustc};

fn finalized_sessions(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for crate_dir in fs::read_dir(root).unwrap() {
        let crate_dir = crate_dir.unwrap().path();
        if !crate_dir.is_dir() {
            continue;
        }
        for session in fs::read_dir(crate_dir).unwrap() {
            let session = session.unwrap().path();
            if session.is_dir()
                && session.file_name().unwrap().to_str().unwrap().starts_with("s-")
                && !session.file_name().unwrap().to_str().unwrap().ends_with("-working")
            {
                result.push(session);
            }
        }
    }
    result.sort();
    result
}

fn main() {
    cc().input("fault.c").arg("-shared").arg("-fPIC").arg("-ldl").out_exe("fault.so").run();
    let interposer = fs::canonicalize("fault.so").unwrap();
    // Dedicated filesystem validation sets this to distinguish actual reflink appends from
    // fallback coverage. Ordinary test runs work on either kind of filesystem.
    let expect_clone = std::env::var("RUSTC_TEST_EXPECT_QUERY_CACHE_CLONE").ok();
    let source = (0..100)
        .map(|i| format!("pub fn cached_{i}(x: usize) -> usize {{ x + {i} }}\n"))
        .collect::<String>();

    for mode in ["write", "kill", "rename", "finalize"] {
        fs::write("cache.rs", &source).unwrap();
        let root = PathBuf::from(format!("incr-{mode}"));
        let compile = || {
            let mut command = rustc();
            command
                .input("cache.rs")
                .crate_type("rlib")
                .emit("metadata")
                .incremental(&root)
                .arg("-Zohm-cloned-query-cache-prefix=yes")
                .arg("-Zincremental-info")
                .arg("-Zincremental-verify-ich")
                .env("LD_PRELOAD", &interposer);
            command
        };
        compile().run();
        let previous = finalized_sessions(&root);
        assert_eq!(previous.len(), 1);
        let cache = previous[0].join("query-cache.bin");
        let donor = PathBuf::from(format!("donor-{mode}.bin"));
        fs::hard_link(&cache, &donor).unwrap();
        let original = fs::read(&donor).unwrap();
        fs::write("cache.rs", source.replace("x + 99", "x + 100")).unwrap();

        let mut command = compile();
        command.env("QUERY_CACHE_FAULT", mode);
        let output =
            if matches!(mode, "write" | "kill") { command.run_fail() } else { command.run() };
        fs::write(format!("{mode}.stderr"), output.stderr()).unwrap();
        match expect_clone.as_deref() {
            Some("1") if mode == "rename" => {
                output.assert_stderr_contains("query-cache-fault: rename");
            }
            Some("1") => {
                output.assert_stderr_contains("copy-on-write prefix bytes");
            }
            Some("0") => {
                output.assert_stderr_contains("prefix bytes after clone failure");
                output.assert_stderr_not_contains("copy-on-write prefix bytes");
            }
            None => {}
            _ => panic!("RUSTC_TEST_EXPECT_QUERY_CACHE_CLONE must be 0 or 1"),
        }
        match mode {
            "write" => {
                output.assert_stderr_contains("failed to write query cache");
                assert_eq!(finalized_sessions(&root), previous);
            }
            "kill" => {
                assert_eq!(output.status().code(), Some(86));
                assert_eq!(finalized_sessions(&root), previous);
            }
            "rename" => {
                // On filesystems without cloning, the normal fallback is selected before
                // reaching the rename. Both cases must finish without changing the donor.
                output.assert_stderr_contains("prefix bytes after clone failure");
            }
            "finalize" => {
                output.assert_stderr_contains(
                    "did not finalize incremental compilation session directory",
                );
                assert_eq!(finalized_sessions(&root), previous);
            }
            _ => unreachable!(),
        }
        assert_eq!(fs::read(&donor).unwrap(), original);

        // Incomplete working directories and failed appends must not poison the next build.
        // The old cache remains readable even when the optimization is switched off.
        compile().arg("-Zohm-cloned-query-cache-prefix=no").arg("-Zassert-incr-state=loaded").run();
        assert_eq!(fs::read(&donor).unwrap(), original);
    }
}
