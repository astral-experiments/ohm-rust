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

    dependency().metadata("new-dependency").cfg("changed").run();
    subject("third", "metadata", Some("loaded")).run_fail().assert_stderr_contains("E0308");
}
