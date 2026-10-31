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
}
