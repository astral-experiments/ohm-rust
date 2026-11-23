//@ ignore-cross-compile

use run_make_support::{rfs, rustc};

fn main() {
    // Move the function to the next line without moving the assertion or
    // changing the byte distance from the assertion to the function.
    for (revision, padding) in [("original", " "), ("moved", "\n")] {
        let source = format!(
            r#"#![feature(rustc_attrs)]
#![allow(internal_features)]
#[rustc_clean(cfg = "moved", loaded_from_disk = "typeck_root")]
{padding}pub fn read(&value: &u32) -> u32 {{ value }}

// Without another change, the module can be reused without loading `read`'s typeck results.
#[cfg(moved)]
pub fn unrelated() {{}}
"#
        );
        rfs::write("read.rs", source);
        rustc()
            .input("read.rs")
            .crate_type("lib")
            .cfg(revision)
            .incremental("incr")
            .arg("-Zquery-dep-graph")
            .arg("-Zincremental-verify-ich")
            .run();
    }
}
