//@ ignore-cross-compile
//@ needs-dynamic-linking

use run_make_support::{dynamic_lib_name, rfs, run, rustc};

fn main() {
    rustc().input("observe.rs").crate_type("proc-macro").edition("2024").run();
    let template = rfs::read_to_string("template.rs");
    check(&template, "direct");
    check(&template.replace("impl Subject", "#[observe::wrap]\nimpl Subject"), "wrapped");
}

fn check(template: &str, incr: &str) {
    for (revision, prefix, earlier, before_attr) in [
        // Replace one byte with a newline to move the entire impl without
        // also changing the impl attribute's byte offset in its module.
        ("initial", " ", "fn earlier() {}", ""),
        ("prefixed", "\n", "fn earlier() {}", ""),
        ("moved", "\n", "fn earlier() {\n\n    }", ""),
        ("changed_gap", "\n", "fn earlier() {\n\n    }", "\n    "),
        ("changed_column", "\n", "fn earlier() {\n\n    }", "\n        "),
    ] {
        let source = template
            .replace("struct Subject;", &format!("{prefix}struct Subject;"))
            .replace("fn earlier() {}", earlier)
            .replace("#[observe::site]", &format!("{before_attr}#[observe::site]"));
        let (line, text) =
            source.lines().enumerate().find(|(_, s)| s.contains("#[observe::site]")).unwrap();
        let column = text.find('#').unwrap() + 1;
        rfs::write("main.rs", &source);
        rustc()
            .input("main.rs")
            .edition("2024")
            .extern_("observe", dynamic_lib_name("observe"))
            .incremental(incr)
            .cfg(revision)
            .arg("-Zquery-dep-graph")
            .run();
        run("main").assert_stdout_equals(format!("{} {column}", line + 1));
    }
}
