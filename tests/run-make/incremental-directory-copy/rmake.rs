use std::fs;
use std::path::{Path, PathBuf};

use run_make_support::rustc;

fn main() {
    fs::write("empty.rs", "#![feature(no_core)]\n#![no_core]\n").unwrap();
    let compile = |state: &str| {
        rustc()
            .input("empty.rs")
            .crate_type("rlib")
            .emit("metadata")
            .incremental("incr")
            .arg(format!("-Zassert-incr-state={state}"))
            .run()
    };

    compile("not-loaded");
    let first = session_dir();
    fs::write(first.join("sentinel"), "previous session").unwrap();

    // Rust reads reusable artifacts from the locked old session. Unused files stay there.
    compile("loaded");
    let second = session_dir();
    assert_ne!(first, second);
    assert!(!second.join("sentinel").exists());

    // A finalized-session name is not enough: a regular file must not be accepted
    // as reusable incremental state. Retain its lock file so the source passes locking.
    fs::rename(&second, "saved-session").unwrap();
    fs::write(&second, "not a directory").unwrap();
    compile("not-loaded");
    let third = session_dir();
    assert_ne!(third, first);
    assert_ne!(third, second);
}

fn session_dir() -> PathBuf {
    fn directories(path: &Path) -> impl Iterator<Item = PathBuf> {
        fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.file_type().unwrap().is_dir())
            .map(|entry| entry.path())
    }

    let crate_dir = directories(Path::new("incr")).next().unwrap();
    // GC can retain the newest prior session alongside the current one. Session names
    // start with fixed-width timestamps, so the newest directory sorts last.
    directories(&crate_dir).max().unwrap()
}
