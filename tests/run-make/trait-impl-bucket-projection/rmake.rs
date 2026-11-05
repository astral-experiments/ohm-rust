//@ ignore-cross-compile
//@ needs-target-std

use run_make_support::{run, rust_lib_name, rustc};

fn main() {
    for (cfgs, expected) in [
        (&[][..], "3 3 3 3 3"),
        (&["unrelated"], "3 3 3 3 3"),
        (&["unrelated", "matching", "finite"], "7 3 11 13 17"),
        (&["unrelated", "moved"], "3 7 3 3 3"),
        (&["unrelated"], "3 3 3 3 3"),
        (&["unrelated", "moved"], "3 7 3 3 3"),
        (&["unrelated", "retired"], "3 3 3 3 3"),
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

    check_rebound_owner();
}

fn check_rebound_owner() {
    for peer in ["left", "right"] {
        rustc().input("peer.rs").crate_name(peer).crate_type("rlib").arg("-Zpublic-api-hash").run();
    }
    // The supplier source stays identical. Only the crate behind `selected` changes.
    for (peer, expected) in [("left", "19 3"), ("right", "3 19")] {
        rustc()
            .input("roster.rs")
            .crate_type("rlib")
            .cfg("foreign_peer")
            .extern_("selected", rust_lib_name(peer))
            .arg("-Zpublic-api-hash")
            .run();
        for (solver, next) in [("default", false), ("next", true)] {
            for cache in [format!("rebound-{solver}"), format!("fresh-{solver}-{peer}")] {
                let mut reader = rustc();
                reader
                    .input("rebound.rs")
                    .extern_("roster", rust_lib_name("roster"))
                    .extern_("left", rust_lib_name("left"))
                    .extern_("right", rust_lib_name("right"))
                    .incremental(cache)
                    .arg("-Zpublic-api-hash")
                    .arg("-Zincremental-verify-ich");
                if next {
                    reader.arg("-Znext-solver");
                }
                reader.run();
                run("rebound").assert_stdout_equals(expected);
            }
        }
    }
}
