//@ only-unix
//@ needs-dynamic-linking
//@ ignore-cross-compile

use run_make_support::{dynamic_lib_name, is_darwin, rfs, run, rustc, target};

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

fn build_rust_dylib(source: &str, expected: &str) -> String {
    rfs::write("dylib/hidden.rs", source);
    rustc()
        .input("dylib/leaf.rs")
        .crate_type("rlib")
        .edition("2024")
        .incremental("leaf-incremental")
        .opt_level("1")
        .arg("-Zpublic-api-hash")
        .run();
    rustc()
        .input("dylib/owner.rs")
        .crate_type("dylib")
        .edition("2024")
        .extern_("leaf", "libleaf.rlib")
        .incremental("owner-incremental")
        .arg("-Cprefer-dynamic")
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich")
        .run();
    rustc()
        .input("dylib/client.rs")
        .edition("2024")
        .arg("-Ldependency=.")
        .arg("-Cprefer-dynamic")
        .extern_("owner", dynamic_lib_name("owner"))
        .run();
    run("client").assert_stdout_equals(expected);
    rustc().arg("-Zls=public_hash").input("libleaf.rlib").run().stdout_utf8()
}

fn main() {
    if !target().contains("-linux-") && !is_darwin() {
        return;
    }
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

    let source = rfs::read_to_string("dylib/hidden.rs");
    let old = build_rust_dylib(&source, "17\n");
    assert_eq!(build_rust_dylib(&source.replace("value: u32", "value: u64"), "4294967313\n"), old);
    assert_eq!(build_rust_dylib(&source, "17\n"), old);
}
