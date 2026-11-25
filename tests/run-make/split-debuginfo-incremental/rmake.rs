//@ needs-asm-support
//@ ignore-cross-compile

// Regression test for <https://github.com/rust-lang/rust/issues/161824>.
// Retained debug objects must not accumulate on cached or changed incremental builds,
// and publishing them must not corrupt hard-linked incremental work products.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use run_make_support::{is_darwin, llvm_dwarfdump, rfs, run, run_in_tmpdir, rustc, target};

fn debug_objects() -> BTreeSet<PathBuf> {
    rfs::read_dir(".")
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            let name = path.file_name().unwrap().to_str().unwrap();
            name.ends_with(".rcgu.o") || name.ends_with(".rcgu.dwo")
        })
        .collect()
}

fn main() {
    if !is_darwin() && !target().contains("linux") {
        return;
    }

    if target().contains("linux") {
        for kind in ["split", "single"] {
            run_in_tmpdir(|| check_reused_debug_files(kind));
        }
    }
    run_in_tmpdir(check_rebuilds);
    run_in_tmpdir(|| check_saved_link_state("-Zemit-rlink"));
    run_in_tmpdir(|| check_saved_link_state("-Zno-link"));
}

/// A relinkable object still names its original split debug file after a cache hit.
fn check_reused_debug_files(kind: &str) {
    for _ in 0..3 {
        // Cargo replaces the relink directory before a full rebuild.
        rfs::create_dir("objects");
        rustc()
            .input("main.rs")
            .output("test")
            .incremental("incr")
            .debuginfo("2")
            .split_debuginfo("unpacked")
            .arg(format!("-Zsplit-dwarf-kind={kind}"))
            .args(["-Zemit-rlink", "-Ztemps-dir=objects"])
            .run();
        let info = llvm_dwarfdump()
            .input("test")
            .arg("--debug-info")
            .arg("--recurse-depth=0")
            .run()
            .stdout_utf8();
        let mut names = info.lines().filter(|line| line.contains("dwo_name")).peekable();
        assert!(names.peek().is_some());
        for line in names {
            let path = line.split('"').nth(1).unwrap();
            assert!(Path::new(path).is_file(), "missing debug file {path}");
        }
        rfs::remove_dir_all("objects");
    }
}

fn check_rebuilds() {
    let compile = || {
        let mut rustc = rustc();
        rustc
            .input("main.rs")
            .output("test")
            .incremental("incr")
            .codegen_units(2)
            .debuginfo("2")
            .split_debuginfo("unpacked");
        rustc
    };

    compile().run();
    run("test");
    let original = debug_objects();
    assert!(!original.is_empty());

    // Reused objects can be hard links to the same inode as the previous output.
    // A rename alone would leave both names behind on Unix.
    for _ in 0..3 {
        compile().run();
        run("test");
        assert_eq!(debug_objects(), original);
    }

    // A backend failure must not modify the last successful incremental session.
    compile().cfg("second").cfg("broken").run_fail();
    compile().run();
    run("test");

    // Failed codegen can leave temporary files. Remove only those from the failed
    // invocation, then check that successful edits still reuse the published names.
    for temporary in debug_objects().difference(&original) {
        rfs::remove_file(temporary);
    }
    for _ in 0..3 {
        compile().cfg("second").run();
        run("test");
        assert_eq!(debug_objects(), original);
        compile().run();
        run("test");
        assert_eq!(debug_objects(), original);
    }
}

fn check_saved_link_state(flag: &str) {
    let compile = || {
        let mut rustc = rustc();
        rustc
            .input("main.rs")
            .output("test")
            .incremental("incr")
            .debuginfo("2")
            .split_debuginfo("unpacked")
            .cfg("print_value")
            .arg(flag);
        rustc
    };
    compile().run();
    rfs::copy("test.rlink", "saved.rlink");
    compile().cfg("second").run();
    rustc().arg("-Zlink-only").input("saved.rlink").run();
    run("test").assert_stdout_equals("1\n");
}
