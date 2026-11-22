//@ needs-target-std

use run_make_support::{Rustc, rfs, rustc};

fn check(cache: &str) -> Rustc {
    let mut cmd = rustc();
    cmd.input("lib.rs")
        .edition("2024")
        .crate_type("rlib")
        .emit("metadata")
        .incremental(cache)
        .arg("-Zohm-shared-stages")
        .arg("-Zincremental-verify-ich")
        .env("RUSTC_LOG", "ohm_module_syntax=info");
    cmd
}

fn main() {
    // Cross the minimum file size for caching.
    let source = format!(
        "// {}\n{}",
        "x".repeat(1024),
        r#"
#[derive(Clone)]
pub struct Item;
#[expect(non_snake_case)]
pub fn BadName(r#async: Item) -> Item { r#async.clone() }
#[cfg(test)]
mod tests {
    fn bad() { unresolved(); }
}
"#
    );
    rfs::write("cached.rs", &source);
    for (step, prefix) in ["", "#[allow(unused)] mod first {}", ""].iter().enumerate() {
        // Move the cached file and its attribute IDs, then move them back.
        rfs::write("lib.rs", format!("#![deny(warnings)]\n{prefix}\npub mod cached;"));
        let result = check("warm").run();
        assert_eq!(result.stderr_utf8().contains("reused parsed module"), step > 0);
        let metadata = rfs::read("liblib.rmeta");
        check(&format!("fresh{step}")).run();
        assert!(metadata == rfs::read("liblib.rmeta"));
    }
    check("warm")
        .cfg("test")
        .run_fail()
        .assert_stderr_contains("cannot find function `unresolved`");

    // Editing inactive code still has to pass the parser.
    rfs::write("cached.rs", source.replace("unresolved();", "let = 0;"));
    check("warm").run_fail().assert_stderr_contains("expected pattern");

    // A statement, an expression and its path segment each have a dummy
    // node ID. Encoding those five-byte IDs would exceed the snapshot limit.
    // Also exercise the first span length that needs the wide representation.
    let name = "d".repeat(255);
    rfs::write(
        "cached.rs",
        format!("#[allow(path_statements)] pub fn {name}(a: u32) {{ {} }}", "a;".repeat(1200)),
    );
    check("dense").run();
    let metadata = rfs::read("liblib.rmeta");
    check("dense").run().assert_stderr_contains("reused parsed module");
    assert!(metadata == rfs::read("liblib.rmeta"));

    // Overlapping token streams would make a snapshot quadratic in the
    // depth of the annotated items. The encoder should abandon this file.
    let nested = format!(
        "#[cfg(any())] {} fn leaf() {{ {} }} {}",
        "#[discard] mod m { ".repeat(32),
        "next(1, 2);".repeat(200),
        "}".repeat(32),
    );
    rfs::write("cached.rs", nested);
    check("nested").run();
    assert!(!check("nested").run().stderr_utf8().contains("reused parsed module"));

    // Pre-expansion linting checks tool names even inside an attribute
    // expression in a disabled module. Change just the tool registration.
    let with_lint = source.replace(
        "fn bad() { unresolved(); }",
        r#"#[doc = { #[allow(custom::lint)] let s = ""; s }] fn bad() {}"#,
    );
    rfs::write("cached.rs", with_lint);
    rfs::write("lib.rs", "#![feature(register_tool)]\n#![register_tool(custom)]\npub mod cached;");
    check("tool").run();
    rfs::write("lib.rs", "#![feature(register_tool)]\npub mod cached;");
    check("tool")
        .run_fail()
        .assert_stderr_contains("reused parsed module")
        .assert_stderr_contains("error[E0710]");

    // The parser builds a long method chain iteratively. Recursive snapshot
    // decoding used to overflow the stack before cfg could remove the item.
    let chain = ".method()".repeat(1024);
    rfs::write("cached.rs", format!("#[cfg(any())] fn unused() {{ 0{chain}; }}"));
    rfs::write("lib.rs", "pub mod cached;");
    check("deep").run();
    rfs::write("lib.rs", "pub mod cached; pub const N: u8 = 0;");
    check("deep").run().assert_stderr_not_contains("reused parsed module");
}
