use run_make_support::{rfs, rustc};

fn main() {
    // Several attributed fields used to require a walk of the whole owner.
    for ty in ["u8", "u32"] {
        rfs::write(
            "lib.rs",
            format!(
                r#"#![deny(missing_docs)]
//! Example.
/// A container.
pub struct Container {{
    #[doc(hidden)]
    pub hidden: {ty},
    /// The visible field.
    pub visible: bool,
}}
"#
            ),
        );
        rustc()
            .input("lib.rs")
            .crate_type("lib")
            .emit("metadata")
            .incremental("fields")
            .arg("-Zincremental-verify-ich")
            .arg("-Zquery-dep-graph")
            .arg("-Zdump-dep-graph")
            .env("RUST_DEP_GRAPH", "graph")
            .run();
        let graph = rfs::read_to_string("graph.txt");
        assert!(!graph.lines().any(|edge| edge == "shallow_lint_levels_on -> hir_owner"));
    }

    // An outer forbid must already be in effect when the inner scope is visited.
    for (level, force) in [("warn", true), ("warn", false), ("forbid", false)] {
        rfs::write(
            "lib.rs",
            format!(
                r#"#[{level}(unused_variables)]
pub fn value(x: Option<u32>) -> Option<u32> {{
    #[allow(unused_variables)]
    let unused = x?;
    x
}}
"#
            ),
        );
        let mut command = rustc();
        command
            .input("lib.rs")
            .crate_type("lib")
            .emit("metadata")
            .incremental("body")
            .arg("-Zincremental-verify-ich");
        if force {
            command
                .arg("--force-warn=unused-variables")
                .run()
                .assert_stderr_contains("unused variable: `unused`");
        } else if level == "forbid" {
            command.run_fail().assert_stderr_contains("error[E0453]");
        } else {
            command.run();
        }
    }
}
