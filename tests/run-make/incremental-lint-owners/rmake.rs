use run_make_support::{rfs, rustc};

fn main() {
    check_large_method();

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

fn check_large_method() {
    let source = r#"#![deny(unfulfilled_lint_expectations)]
pub mod data { pub struct Token; pub fn token() -> Token { Token } }
pub struct Wrapper<T>(pub T);
pub trait Marker {}
#[allow(unused_comparisons)]
impl<T> Wrapper<T> {
    pub fn other() {}
    #[expect(non_local_definitions)]
    pub fn inspect<U: Copy>(&self, byte: u8, _: U) -> bool {
        impl Marker for String {}
        let _ = || {
            mod nested {
                pub fn bytes() -> impl Copy {
                    let bytes: [u8; 1] = [const { 0 }];
                    bytes
                }
            }
        };
        let _: i8 = -128;
        let _ = [VALUES];
        let _ = data::token();
        byte > 255
    }
}
"#
    .replace("VALUES", &"0,".repeat(160));
    let check = |text: &str| {
        rfs::write("lib.rs", text);
        let mut cmd = rustc();
        cmd.input("lib.rs")
            .edition("2024")
            .crate_type("lib")
            .emit("metadata")
            .incremental("methods")
            .arg("-Zincremental-verify-ich")
            .arg("-Zquery-dep-graph")
            .arg("-Zdump-dep-graph")
            .env("RUST_DEP_GRAPH", "graph");
        cmd
    };
    check(&source).run();
    let graph = rfs::read_to_string("graph.txt");
    for edge in [
        "lint_mod -> lint_fn",
        "check_mod_privacy -> check_fn_name_privacy",
        "check_mod_privacy -> check_fn_type_privacy",
        "check_mod_unstable_api_usage -> check_fn_unstable_api_usage",
    ] {
        assert!(graph.lines().any(|line| line == edge), "{edge}");
    }

    // Changing another method must still fulfill an inherited expectation.
    let expected = source.replace("allow(unused_comparisons)", "expect(unused_comparisons)");
    check(&expected).run();
    let moved = expected.replace("other() {}", "other() {\n    let _ = 1;\n    }");
    check(&moved).run();
    // The closure keeps its definition path but gets a different local HIR id.
    let moved = moved.replace("let _ = ||", "let _: i8 = 0; let _ = ||");
    check(&moved).run();
    check(&moved.replace("expect(unused_comparisons)", "deny(unused_comparisons)"))
        .run_fail()
        .assert_stderr_contains("comparison is useless due to type limits");

    // Changing the visibility of an inferred type must recheck the unchanged body.
    check(&moved).run();
    check(&moved.replace("pub struct Token", "struct Token"))
        .run_fail()
        .assert_stderr_contains("type `Token` is private");

    // Collect even an unused module inside a closure, including its opaque type and consts.
    check(&moved).run();
    check(&moved.replace("[u8; 1]", "[bool; 1]")).run_fail().assert_stderr_contains("error[E0308]");
}
