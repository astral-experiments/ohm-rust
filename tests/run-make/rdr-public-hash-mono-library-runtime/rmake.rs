//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn public_hash() -> String {
    rustc()
        .arg("-Zls=public_hash")
        .input("liblibrary.rlib")
        .run()
        .stdout_utf8()
        .lines()
        .find_map(|line| line.strip_prefix("Public hash:"))
        .map(|hash| hash.trim().to_owned())
        .expect("public hash should be present")
}

fn compile_library(changed: bool) {
    let mut command = rustc();
    command
        .input("library.rs")
        .incremental("library")
        .arg("-Zpublic-api-hash")
        .arg("-Zquery-dep-graph")
        .arg("-Zincremental-verify-ich");

    if changed {
        command.cfg("changed");
    }

    command.run();
}

fn compile_runner() {
    rustc()
        .input("main.rs")
        .arg("-Zpublic-api-hash")
        .extern_("library", "liblibrary.rlib")
        .extern_("dependent", "libdependent.rlib")
        .run();
}

fn main() {
    compile_library(false);
    let original_hash = public_hash();

    rustc()
        .input("dependent.rs")
        .arg("-Zpublic-api-hash")
        .extern_("library", "liblibrary.rlib")
        .run();
    let original_dependent = rfs::read("libdependent.rlib");

    compile_runner();
    run("main").assert_stdout_equals("2\n");

    compile_library(true);
    assert_eq!(original_hash, public_hash());
    assert_eq!(original_dependent, rfs::read("libdependent.rlib"));

    compile_runner();
    run("main").assert_stdout_equals("1\n");

    for (incremental_directory, rustflags) in [
        ("closure-error", ["-Zpublic-api-hash", "-Zohm-incremental-mono-partitions=no"]),
        (
            "closure-error-independent-partitions",
            ["-Zpublic-api-hash=no", "-Zohm-incremental-mono-partitions"],
        ),
    ] {
        rustc()
            .input("closure_error.rs")
            .incremental(incremental_directory)
            .args(rustflags)
            .arg("-Zincremental-verify-ich")
            .run();

        rustc()
            .input("closure_error.rs")
            .incremental(incremental_directory)
            .args(rustflags)
            .arg("-Zincremental-verify-ich")
            .cfg("fail")
            .run_fail()
            .assert_stderr_contains("evaluation panicked: required closure constant")
            .assert_stderr_contains("the above error was encountered while instantiating `fn")
            .assert_stderr_contains("trigger::<u8>");
    }
}
