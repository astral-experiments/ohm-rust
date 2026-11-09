//@ ignore-cross-compile
//@ needs-target-std

use run_make_support::{rfs, run, rust_lib_name, rustc};

const OVERLAP: &str = "metadata_export_prefetch_overlap";

fn build(threads: u32, incremental: &str, expected: &str) -> Vec<u8> {
    let output = rustc()
        .input("provider.rs")
        .crate_type("rlib")
        .emit("metadata,link")
        .incremental(incremental)
        .arg(format!("-Zthreads={threads}"))
        .arg("-Zshare-generics=yes")
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich")
        .arg("-Ztime-passes")
        .arg("-Ztime-passes-format=json")
        .run();
    assert_eq!(output.stderr_utf8().contains(OVERLAP), threads > 1);

    rustc()
        .input("main.rs")
        .extern_("provider", rust_lib_name("provider"))
        .arg("-Zshare-generics=yes")
        .run();
    run("main").assert_stdout_equals(expected);
    rfs::read("libprovider.rmeta")
}

fn main() {
    rustc()
        .input("provider.rs")
        .crate_type("rlib")
        .emit("metadata")
        .arg("-Zthreads=4")
        .arg("-Ztime-passes")
        .arg("-Ztime-passes-format=json")
        .run()
        .assert_stderr_not_contains(OVERLAP);

    let initial = build(1, "incr-a", "9 5 4");
    assert!(initial == build(4, "incr-b", "9 5 4"), "initial metadata differs");

    let source = rfs::read_to_string("provider.rs");
    rfs::write("provider.rs", source.replace("7u16", "7u8"));
    let edited = build(4, "incr-a", "8 5 4");
    assert!(initial != edited, "the export change must invalidate the metadata");
    assert!(edited == build(1, "incr-b", "8 5 4"), "edited metadata differs");
}
