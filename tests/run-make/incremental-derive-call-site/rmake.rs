//@ ignore-cross-compile
//@ needs-dynamic-linking

use run_make_support::{dynamic_lib_name, rfs, run, rustc};

fn main() {
    rustc().input("observe.rs").crate_type("proc-macro").edition("2024").run();
    let source = rfs::read_to_string("template.rs");
    for (revision, value, anchor) in [
        ("initial", "1", "struct Anchor;"),
        // This moves the derive's byte offset while preserving its line and its input.
        ("after_bytes", "12345", "struct Anchor;"),
        // The previous anchor disappears, and the native macro must observe its new line.
        ("after_lines", "12345", "\n\n"),
    ] {
        let source =
            source.replace("= 1;", &format!("= {value};")).replace("struct Anchor;", anchor);
        let line = 1 + source.lines().position(|s| s.starts_with("#[derive(")).unwrap();
        rfs::write("main.rs", source);
        rustc()
            .input("main.rs")
            .edition("2024")
            .extern_("observe", dynamic_lib_name("observe"))
            .incremental("incr")
            .cfg(revision)
            .arg("-Zquery-dep-graph")
            .run();
        run("main").assert_stdout_equals(line.to_string());
    }
}
