//@ needs-target-std
//
// This is a simple smoke test for rustc's `--emit dep-info` feature. It prints out
// information about dependencies in a Makefile-compatible format, as a `.d` file.
// On Unix it also checks that Make recognizes a source file with literal metacharacters.

// This test first checks that emitting dep-info disables static analysis, preventing
// compilation of `erroneous.rs` from causing a compilation failure.
// Then, it checks that compilation using the flag is successful in general, even with
// empty source files or source files that contain a whitespace character.

// Finally, it removes one dependency and checks that compilation is still successful.
// See https://github.com/rust-lang/rust/pull/10698

use run_make_support::{cmd, is_windows, path, rfs, rustc};

fn main() {
    // We're only emitting dep info, so we shouldn't be running static analysis to
    // figure out that this program is erroneous.
    rustc().input("erroneous.rs").emit("dep-info").out_dir("fresh/nested").run();
    assert!(path("fresh/nested/erroneous.d").is_file());

    rustc()
        .input("erroneous.rs")
        .emit("dep-info=./relative/name.d")
        .out_dir(path("relative"))
        .run();
    assert!(path("relative/name.d").is_file());

    rustc().input("erroneous.rs").emit("dep-info=custom.d").out_dir("unused").run();
    assert!(path("custom.d").is_file());
    assert!(!path("unused").exists());

    rustc().input("lib.rs").emit("dep-info,link").crate_type("lib").run();
    rfs::remove_file("foo.rs");
    rfs::create_file("foo.rs");
    // Compilation should succeed even if `foo.rs` is empty.
    rustc().input("lib.rs").emit("dep-info,link").crate_type("lib").run();

    // Again, with a space in the filename this time around.
    rustc().input("lib_foofoo.rs").emit("dep-info,link").crate_type("lib").run();
    rfs::remove_file("foo foo.rs");
    rfs::create_file("foo foo.rs");
    // Compilation should succeed even if `foo foo.rs` is empty.
    rustc().input("lib_foofoo.rs").emit("dep-info,link").crate_type("lib").run();

    // When a source file is deleted, compilation should still succeed if the library
    // also loses this source file dependency.
    rfs::remove_file("bar.rs");
    rustc().input("lib2.rs").emit("dep-info,link").crate_type("lib").run();

    if !is_windows() {
        let input = "cash$X #tag\t.txt";
        let output = "lib #$X.rmeta";
        rfs::write(input, "tracked");
        rfs::write(
            "make_paths.rs",
            r##"pub const VALUE: &str = include_str!("cash$X #tag\t.txt");"##,
        );
        rustc()
            .input("make_paths.rs")
            .crate_type("lib")
            .emit(format!("dep-info=paths.d,metadata={output}"))
            .run();
        rfs::write("Makefile", "X :=\nlib\\ \\#$$X.rmeta:\n\t@echo rebuild\ninclude paths.d\n");
        let rules = rfs::read_to_string("paths.d");
        let marker = "/dev/null/ohm-ambiguous-gcc-path";
        assert!(rules.lines().any(|line| line.ends_with(&format!(": | {marker}"))), "{rules}");
        assert!(rules.lines().any(|line| line == format!("{marker}:")), "{rules}");
        let probe =
            |changed: &str| cmd("make").args(&["-s", "-W", changed, output]).run().stdout_utf8();
        assert_eq!(probe(input).trim(), "rebuild");
        assert!(probe("unrelated.txt").is_empty());
    }
}
