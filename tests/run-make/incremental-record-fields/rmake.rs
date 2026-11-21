use run_make_support::{rfs, rustc};

// A sibling rename doesn't affect a read of `data`. Swapping two fields
// of the same type must still update the field index in the const fn.
fn main() {
    let library = |directory: &str| {
        let mut cmd = rustc();
        cmd.input("fields.rs")
            .edition("2024")
            .crate_type("lib")
            .emit("metadata")
            .env("CARGO_PKG_NAME", "fields")
            .metadata("fixed")
            .arg("-Zohm-shared-stages")
            .arg("-Zpublic-api-hash")
            .arg("-Zincremental-verify-ich");
        cmd.arg("-Zquery-dep-graph").incremental(directory);
        cmd
    };
    library("record").run();
    library("record").cfg("renamed").cfg("reuse").run();

    let swapped = |directory: &str| {
        let mut cmd = library(directory);
        cmd.cfg("renamed").cfg("swapped");
        cmd
    };
    swapped("record").run();
    let warm = rfs::read("libfields.rmeta");
    swapped("record-fresh").run();
    assert!(warm == rfs::read("libfields.rmeta"));

    swapped("record").cfg("hidden").run_fail().assert_stderr_contains("E0616");
    swapped("record-fresh").cfg("removed").run_fail().assert_stderr_contains("E0609");
}
