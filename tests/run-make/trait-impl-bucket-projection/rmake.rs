//@ ignore-cross-compile
//@ needs-target-std

use run_make_support::{run, rust_lib_name, rustc};

fn main() {
    for (cfgs, expected) in [
        (&[][..], "3 3"),
        (&["unrelated"], "3 3"),
        (&["unrelated", "matching"], "7 3"),
        (&["unrelated", "moved"], "3 7"),
        (&["unrelated"], "3 3"),
        (&["unrelated", "moved"], "3 7"),
        (&["unrelated", "retired"], "3 3"),
    ] {
        let mut upstream = rustc();
        upstream.input("roster.rs").crate_type("rlib");
        for &cfg in cfgs {
            upstream.cfg(cfg);
        }
        upstream.run();

        for (cache, next_solver) in [("default", false), ("next", true)] {
            let mut reader = rustc();
            reader
                .input("reader.rs")
                .extern_("roster", rust_lib_name("roster"))
                .incremental(cache)
                .arg("-Zincremental-verify-ich");
            if next_solver {
                reader.arg("-Znext-solver");
            }
            reader.run();
            run("reader").assert_stdout_equals(expected);
        }
    }
}
