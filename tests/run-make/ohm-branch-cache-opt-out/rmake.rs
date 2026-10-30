//@ ignore-cross-compile
// Reason: runs the executable produced by each compiler invocation.

use std::fs;
use std::path::{Path, PathBuf};

use run_make_support::{cwd, rfs, run, rustc};

fn sessions(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect()
}

fn main() {
    rfs::create_dir_all("checkout/.git");
    // Changing only the rustc option must exercise GC in the same crate directory.
    let mut crate_dir = None;
    for (branch, value, option, count) in [
        ("a", "17", None, 1),
        ("b", "29", None, 1),
        ("a", "17", Some(true), 1),
        ("b", "29", Some(true), 2),
        ("a", "17", Some(true), 2),
        ("b", "29", Some(false), 1),
        ("a", "17", None, 1),
    ] {
        rfs::write("checkout/.git/HEAD", format!("ref: refs/heads/{branch}\n"));
        rfs::write("checkout/main.rs", format!("fn main() {{ println!(\"{value}\"); }}"));
        let mut cmd = rustc();
        cmd.input("checkout/main.rs")
            .crate_name("branch_cache")
            .env("CARGO_MANIFEST_DIR", cwd().join("checkout"))
            .arg("-Zohm-check-relink-inputs")
            .incremental("state")
            .output("program");
        if let Some(enabled) = option {
            cmd.arg(if enabled { "-Zohm-branch-cache=yes" } else { "-Zohm-branch-cache=no" });
        }
        cmd.run();
        run("program").assert_stdout_equals(value);
        let [dir]: [_; 1] = sessions(Path::new("state")).try_into().unwrap();
        assert_eq!(crate_dir.get_or_insert(dir.clone()), &dir);
        let dirs = sessions(&dir);
        assert_eq!(dirs.len(), count);
        for entry in dirs {
            let name = entry.file_name().unwrap().to_str().unwrap();
            assert_eq!(name.split('-').nth(2).unwrap().starts_with('_'), option == Some(true));
        }
    }
}
