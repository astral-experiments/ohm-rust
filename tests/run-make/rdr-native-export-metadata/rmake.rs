//@ only-linux
//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn build(source: &str, expected: &str) -> String {
    rfs::write("hidden.rs", source);
    rustc()
        .input("library.rs")
        .edition("2024")
        .incremental("producer")
        .arg("-Zpublic-api-hash")
        .run();
    rustc()
        .input("main.rs")
        .edition("2024")
        .extern_("library", "liblibrary.rlib")
        .incremental("consumer")
        .arg("-Zincremental-verify-ich")
        .run();
    run("main").assert_stdout_equals(expected);
    rustc().arg("-Zls=public_hash").input("liblibrary.rlib").run().stdout_utf8()
}

fn main() {
    let source = rfs::read_to_string("hidden.rs");
    let old = build(&source, "(17, 17, 17)\n");
    for (before, after, expected) in [
        ("native_hook_aaaa", "native_hook_bbbb", "(17, 17, 17)\n"),
        ("scalar(x: u32)", "scalar(x: u64)", "(17, 4294967313, 17)\n"),
        ("value: u32", "value: u64", "(17, 17, 4294967313)\n"),
    ] {
        let edited = source.replace(before, after);
        assert_ne!(build(&edited, expected), old);
        assert_eq!(build(&source, "(17, 17, 17)\n"), old);
    }
}
