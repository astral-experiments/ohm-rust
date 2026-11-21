//@ ignore-cross-compile

use run_make_support::{Rustc, recursive_find_files, rfs, run, rustc};

fn subject(salt: &str, emit: &str, state: Option<&str>) -> Rustc {
    let mut command = rustc();
    command
        .input("subject.rs")
        .edition("2024")
        .extern_("randomized", "librandomized.rlib")
        .metadata(salt)
        .emit(emit)
        .env("CARGO_PKG_NAME", "subject")
        .env("CARGO_PKG_VERSION", "1.0.0")
        .arg("-Zohm-shared-stages")
        .arg("-Zincremental-verify-ich");
    if let Some(state) = state {
        command.incremental("incr").arg(format!("-Zassert-incr-state={state}"));
    }
    command
}

fn dependency() -> Rustc {
    let mut command = rustc();
    command
        .input("randomized.rs")
        .crate_type("rlib")
        .arg("-Zrandomize-layout")
        .arg("-Zlayout-seed=1234");
    command
}

fn truncate_check_key() {
    let graphs =
        recursive_find_files("incr", |path| path.file_name().unwrap() == "ohm-check-dep-graph.bin");
    let [graph] = graphs.as_slice() else { panic!("expected one check graph") };
    let mut bytes = rfs::read(graph);
    assert_eq!(&bytes[..4], b"RSIC");
    bytes.truncate(7 + usize::from(bytes[6]) + size_of::<u64>());
    bytes.extend_from_slice(b"rust-end-file");
    rfs::remove_file(graph);
    rfs::write(graph, bytes);
}

fn main() {
    supplier_release();
    changed_binding();
    dependency().run();
    subject("damaged", "metadata", Some("not-loaded")).run();
    truncate_check_key();
    subject("first", "metadata", Some("not-loaded")).run();
    // Reload the local trait object's projection list before changing the linkage identity.
    subject("first", "metadata", Some("loaded")).run();
    subject("second", "metadata", Some("loaded")).run();
    subject("different-cfg", "metadata", Some("not-loaded")).cfg("variant").run();
    subject("fresh-build", "link", Some("not-loaded")).run();

    subject("second", "link", Some("loaded")).run();
    let reused = run("subject").stdout_utf8();
    subject("second", "link", None).run();
    assert_eq!(reused, run("subject").stdout_utf8());

    // A release changes both the metadata salt and the value expanded by env!.
    subject("release", "metadata", Some("loaded")).env("CARGO_PKG_VERSION", "1.0.1").run();
    subject("longer-version", "metadata", Some("loaded"))
        .env("CARGO_PKG_VERSION", "1.0.10")
        .run_fail()
        .assert_stderr_contains("E0080");

    dependency().metadata("new-dependency").cfg("changed").run();
    subject("third", "metadata", Some("loaded")).run_fail().assert_stderr_contains("E0308");
}

fn check_library(name: &str, salt: &str) -> Rustc {
    let mut cmd = rustc();
    cmd.input(format!("{name}.rs"))
        .edition("2024")
        .crate_type("lib")
        .emit("metadata")
        .env("CARGO_PKG_NAME", name)
        .metadata(salt)
        .arg("-Zohm-shared-stages")
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich");
    cmd
}

fn supplier_release() {
    let supplier = |salt: &str| {
        let mut cmd = check_library("supplier", salt);
        cmd.arg("-Zrandomize-layout");
        cmd
    };
    let consumer = |directory: &str| {
        let mut cmd = check_library("consumer", "fixed");
        cmd.incremental(directory);
        cmd.arg("-Zquery-dep-graph").extern_("supplier", "libsupplier.rmeta");
        cmd
    };
    supplier("first").run();
    consumer("consumer-incr").run();
    // Reload an ordered pair of external projections before changing the salt.
    consumer("consumer-incr").run();
    supplier("second").run();
    consumer("consumer-incr").cfg("release").run();

    // Compare the encoded type and randomized layout with a fresh check.
    supplier("third").run();
    consumer("consumer-incr").run();
    let warm = rfs::read("libconsumer.rmeta");
    consumer("fresh").run();
    assert!(warm == rfs::read("libconsumer.rmeta"));

    // The new key still tracks the contents of the upstream definition.
    supplier("changed").cfg("changed").run();
    consumer("consumer-incr").run_fail().assert_stderr_contains("E0308");
}

// A private path changes without altering the canonical metadata of the bridge.
// The reader must interpret the re-export using the new binding.
fn changed_binding() {
    rfs::write("dep.rs", "pub struct Packet;");
    rfs::write("bridge.rs", "pub use dep::Packet;");
    rfs::write("reader.rs", "pub fn read() -> bridge::Packet { bridge::Packet }");

    let dep = |package| {
        check_library("dep", "fixed").env("CARGO_PKG_NAME", package).incremental("dep-incr").run();
    };
    let read = || {
        check_library("reader", "fixed")
            .extern_("bridge", "libbridge.rmeta")
            .incremental("reader-incr")
            .run();
    };
    dep("original");
    check_library("bridge", "fixed").extern_("dep", "libdep.rmeta").run();
    read();
    dep("changed");
    read();
}
