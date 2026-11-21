//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn main() {
    let parse = "fn parse(input: &[u8]) -> Option<&str> {
    if input.is_empty() {
        return None;
    }

    Some(std::str::from_utf8(input).ok()?)
}";

    let assertion = "assert_eq!(parse(b\"hi\"), Some(\"hi\"));";

    // Moving the question mark changes its expansion span but preserves the mono-item edges.
    // Adding a call then makes the collector read those edges from the preceding compilation.
    for (padding, extra_call) in
        [("", ""), ("\n\n", ""), ("\n\n", "std::hint::black_box(17usize);")]
    {
        rfs::write(
            "main.rs",
            format!("{padding}{parse}\nfn main() {{ {assertion} {extra_call} }}"),
        );
        rustc().input("main.rs").incremental("incr").arg("-Zohm-incremental-mono-partitions").run();
        run("main");
    }

    // Keep the assertion's position relative to the function fixed in bytes,
    // but move the question mark (and its generated lint attribute) one line.
    for (revision, padding) in [("original", " "), ("moved", "\n")] {
        rfs::write(
            "lib.rs",
            format!(
                r#"#![feature(rustc_attrs)]
#![allow(internal_features)]
#[rustc_clean(cfg = "moved", loaded_from_disk = "typeck_root")]
{padding}pub {parse}
"#
            ),
        );
        rustc()
            .input("lib.rs")
            .crate_type("lib")
            .cfg(revision)
            .incremental("check")
            .arg("-Zquery-dep-graph")
            .arg("-Zincremental-verify-ich")
            .run();
    }
}
