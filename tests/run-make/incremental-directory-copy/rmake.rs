use std::fs;
use std::path::{Path, PathBuf};

use run_make_support::rustc;

fn main() {
    fs::write("empty.rs", "#![feature(no_core)]\n#![no_core]\n").unwrap();
    let compile = || {
        rustc()
            .input("empty.rs")
            .crate_type("rlib")
            .emit("metadata")
            .incremental("incr")
            .arg("-Zincremental-info")
            .run()
    };

    compile();
    let first = session_dir();
    fs::write(first.join("sentinel"), "previous session").unwrap();

    // Both directory cloning and the per-file fallback must populate the new session.
    compile().assert_stderr_contains("[incremental] session directory");
    let second = session_dir();
    assert_ne!(first, second);
    assert_eq!(fs::read_to_string(second.join("sentinel")).unwrap(), "previous session");

    // A finalized-session name is not enough: cloning a regular file must not be accepted
    // as a successful directory copy. Retain its lock file so the source passes locking.
    fs::rename(&second, "saved-session").unwrap();
    fs::write(&second, "not a directory").unwrap();
    compile().assert_stderr_not_contains("failed to delete partly initialized session dir");
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
    let sessions: Vec<_> = directories(&crate_dir).collect();
    assert_eq!(sessions.len(), 1, "expected only the newest completed session: {sessions:?}");
    sessions.into_iter().next().unwrap()
}
