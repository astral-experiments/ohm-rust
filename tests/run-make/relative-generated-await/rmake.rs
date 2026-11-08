//@ ignore-cross-compile
//@ needs-crate-type: proc-macro

use run_make_support::{Rustc, rfs, run, rustc};

fn build() -> Rustc {
    let mut cmd = rustc();
    cmd.input("main.rs")
        .edition("2024")
        .arg("--extern=attr")
        .library_search_path(".")
        .incremental("incr")
        .arg("-Zquery-dep-graph")
        .arg("-Zincremental-verify-ich");
    cmd
}

fn attribute_line(source: &str, attribute: &str) -> usize {
    source.lines().position(|line| line.trim() == attribute).unwrap() + 1
}

fn main() {
    rustc().input("attr.rs").edition("2024").run();
    let original = rfs::read_to_string("main.rs");
    let shifted = original.replace("// Insert lines", "\n\n    // Insert lines");

    // The generated await's owner can stay green. A separate line!() must still move.
    for (source, revision) in [(&original, "original"), (&shifted, "moved")] {
        rfs::write("main.rs", source);
        build().cfg(revision).run();
        let line = attribute_line(source, "#[add_await(location)]");
        run("main").assert_stdout_equals(format!("7 11 {line} 5\n"));
    }

    rfs::write("main.rs", shifted.replace("7u32", "\"bad\""));
    let line = attribute_line(&shifted, "#[add_await]");
    build()
        .run_fail()
        .assert_stderr_contains("error[E0308]")
        .assert_stderr_contains(format!("--> main.rs:{line}:5"));
}
