// Check that adding a body attribute after an edit reruns the attribute
// checks, including those that apply to an unannotated async block.

use run_make_support::{rfs, rustc};

fn main() {
    rfs::write(
        "lib.rs",
        r#"#![feature(rustc_attrs)]
#![allow(internal_features)]
#![deny(unused_attributes)]
#[cfg_attr(force, rustc_force_inline)]
pub fn value(x: Option<u32>) -> Option<u32> {
    #[allow(dead_code)]
    let n = x?;
    let _ = async {};
    Some(n)
}
"#,
    );
    for revision in ["ordinary", "force", "empty"] {
        if revision == "empty" {
            let source = rfs::read_to_string("lib.rs");
            rfs::write("lib.rs", source.replace("allow(dead_code)", "allow()"));
        }
        let mut command = rustc();
        command
            .input("lib.rs")
            .crate_type("lib")
            .edition("2024")
            .emit("metadata")
            .incremental("coro")
            .arg("-Zincremental-verify-ich")
            .arg("-Zquery-dep-graph")
            .arg("-Zdump-dep-graph")
            .env("RUST_DEP_GRAPH", "graph")
            .cfg(revision);
        match revision {
            "ordinary" => {
                command.run();
                let graph = rfs::read_to_string("graph.txt");
                assert!(!graph.lines().any(|edge| edge == "check_mod_attrs -> def_span"));
            }
            "force" => {
                command.run_fail().assert_stderr_contains("attribute cannot be applied");
            }
            "empty" => {
                command.run_fail().assert_stderr_contains("unused attribute");
            }
            _ => unreachable!(),
        }
    }
}
