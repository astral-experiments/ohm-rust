// Rustc did not recognize libraries which were symlinked
// to files having extension other than .rlib. This was fixed
// in #32828. This test creates a symlink to "foo.xxx", which has
// an unusual file extension, and checks that rustc can successfully
// use it as an rlib library.
// See https://github.com/rust-lang/rust/pull/32828

//@ ignore-cross-compile
//@ needs-symlink

use run_make_support::{cwd, rfs, rustc};

fn main() {
    rustc().input("foo.rs").crate_type("rlib").output("foo.xxx").run();
    rfs::symlink_file("foo.xxx", "libfoo.rlib");
    rustc().input("bar.rs").library_search_path(cwd()).run();

    // Directory aliases and file symlinks can refer to the same candidate.
    rfs::create_dir("libraries");
    rustc().input("foo.rs").crate_type("rlib").output("libraries/libfoo.rlib").run();
    rfs::symlink_dir("libraries", "directory-alias");
    rfs::create_dir("file-alias");
    rfs::symlink_file("../libraries/libfoo.rlib", "file-alias/libfoo.rlib");
    rustc()
        .input("bar.rs")
        .library_search_path("libraries")
        .library_search_path("directory-alias")
        .library_search_path("file-alias")
        .run();

    // Matching filenames in distinct directories must remain distinct candidates.
    rfs::create_dir("other");
    rustc().input("foo.rs").crate_type("rlib").metadata("other").output("other/libfoo.rlib").run();
    rustc()
        .input("bar.rs")
        .library_search_path("libraries")
        .library_search_path("other")
        .run_fail()
        .assert_stderr_contains("multiple candidates for `rlib` dependency `foo` found");
}
