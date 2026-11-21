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
    native_supplier();
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

    revisit_salt();
}

// Check an old package version, change the source, and bump the version
// back. A newer compatible session now has the current crate root.
fn revisit_salt() {
    let check = |salt: &str| {
        let mut cmd = rustc();
        cmd.input("revisit.rs")
            .edition("2024")
            .crate_type("lib")
            .emit("metadata")
            .env("CARGO_PKG_NAME", "revisit")
            .metadata(salt)
            .arg("-Zohm-shared-stages")
            .arg("-Zpublic-api-hash")
            .arg("-Zincremental-verify-ich");
        cmd.arg("-Zquery-dep-graph").cfg("check").incremental("salts");
        cmd
    };
    let root = |ty: &str| {
        rfs::write(
            "revisit.rs",
            format!(
                "#![feature(rustc_attrs)]\n#![allow(internal_features)]\n\
                 pub type Arg = {ty}; pub mod child;"
            ),
        );
    };
    let child = |value: &str, cfg: &str| {
        rfs::write(
            "child.rs",
            format!("#[rustc_clean(cfg = \"{cfg}\")] pub fn value() -> super::Arg {{ {value} }}"),
        );
    };

    root("u8");
    child("1", "never");
    check("a").run();
    root("u16");
    child("2", "never");
    check("b").run();

    // The newest check has the same crate root but incompatible flags.
    child("3", "never");
    check("c").cfg("other").run();
    child("2", "check");
    check("a").run();

    // When both roots match, prefer the original salt.
    child("4", "never");
    check("d").run();
    child("2", "check");
    check("a").run();

    // Matching the root must not hide a change in another file.
    root("u32");
    child("5", "never");
    check("e").run();
    child("false", "never");
    check("a").run_fail().assert_stderr_contains("E0308");
}

fn check_library(name: &str, salt: &str) -> Rustc {
    library(name, salt, "metadata")
}

fn library(name: &str, salt: &str, emit: &str) -> Rustc {
    let mut cmd = rustc();
    cmd.input(format!("{name}.rs"))
        .edition("2024")
        .crate_type("lib")
        .emit(emit)
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

    // Adding an impl for another receiver should preserve the type check of value(Packet).
    supplier("third").cfg("extra").run();
    consumer("consumer-incr").cfg("extra").run();

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

// A codegen session must agree with a check on the keys for imported types.
// Its work products must also agree with the canonical identity of the supplier.
fn native_supplier() {
    rfs::write(
        "api.rs",
        r#"
pub struct Packet;
#[inline(never)]
pub fn read(_: Packet) -> u32 { 40 }
"#,
    );
    let reader = r#"
#![feature(rustc_attrs)]
#![allow(internal_features)]
#![rustc_partition_reused(module = "reader-fixed", cfg = "same")]
#![rustc_partition_codegened(module = "reader-fixed", cfg = "changed")]

pub mod fixed {
    #[inline(never)]
    pub fn value() -> u32 { 1 }
}

#[rustc_clean(cfg = "from_build", loaded_from_disk = "fn_sig")]
pub fn typed(_: api::Packet) {}

#[rustc_clean(
    cfg = "same", except = "hir_owner,optimized_mir", loaded_from_disk = "typeck_root,mir_borrowck"
)]
pub fn value() -> u32 { api::read(api::Packet) + DELTA }

pub const ID: std::any::TypeId = std::any::TypeId::of::<api::Packet>();
"#;
    rfs::write(
        "main.rs",
        r#"
fn main() {
    reader::typed(api::Packet);
    assert_eq!(reader::ID, std::any::TypeId::of::<api::Packet>());
    println!("{}", reader::value());
}
"#,
    );

    for (build, salt, revision, delta) in [
        (true, "a", "initial", 1),
        (false, "a", "from_build", 2),
        (true, "a", "same", 2),
        (false, "b", "check_b", 2),
        (true, "b", "changed", 2),
    ] {
        rfs::write("reader.rs", reader.replace("DELTA", &delta.to_string()));
        let emit = if build { "metadata,link" } else { "metadata" };
        library("api", salt, emit).run();
        library("reader", "fixed", emit)
            .extern_("api", if build { "libapi.rlib" } else { "libapi.rmeta" })
            .incremental("native-incr")
            .codegen_units(16)
            .cfg(revision)
            .arg("-Zquery-dep-graph")
            .run();
        if build {
            rustc()
                .input("main.rs")
                .edition("2024")
                .extern_("api", "libapi.rlib")
                .extern_("reader", "libreader.rlib")
                .run();
            run("main").assert_stdout_equals(format!("{}\n", 40 + delta));
        }
    }
}
