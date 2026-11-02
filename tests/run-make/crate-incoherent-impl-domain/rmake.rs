//@ ignore-cross-compile
//@ needs-target-std

use run_make_support::{run, rust_lib_name, rustc};

fn main() {
    rustc().input("owner.rs").crate_type("rlib").edition("2024").run();

    for (cfgs, expected) in [
        (&[][..], Ok("0 0 0")),
        (&["unrelated"], Ok("0 0 0")),
        (&["string"], Ok("11 0 0")),
        (&["string", "unrelated"], Ok("11 0 0")),
        (&["token"], Ok("0 0 31")),
        (&["token", "shared"], Ok("0 23 31")),
        (&["shared"], Ok("0 23 0")),
        (&["string_argument"], Err("E0061")),
        (&[], Ok("0 0 0")),
        (&["string"], Ok("11 0 0")),
    ] {
        let mut lib = rustc();
        lib.input("roster.rs")
            .edition("2024")
            .crate_type("rlib")
            .extern_("owner", rust_lib_name("owner"));
        for cfg in cfgs {
            lib.cfg(cfg);
        }
        lib.run();
        for (cache, next) in [("ordinary", false), ("next", true)] {
            let mut reader = rustc();
            reader
                .input("reader.rs")
                .edition("2024")
                .extern_("owner", rust_lib_name("owner"))
                .extern_("roster", rust_lib_name("roster"))
                .arg("-Ldependency=.")
                .incremental(cache)
                .arg("-Zincremental-verify-ich");
            if next {
                reader.arg("-Znext-solver");
            }
            match expected {
                Ok(stdout) => {
                    reader.run();
                    run("reader").assert_stdout_equals(stdout);
                }
                Err(code) => {
                    reader.run_fail().assert_stderr_contains(code);
                }
            }
        }
    }

    for (enabled, output) in [(false, "0"), (true, "43"), (false, "0")] {
        let mut local = rustc();
        local.input("local.rs").edition("2024").incremental("local-cache");
        if enabled {
            local.cfg("local");
        }
        local.run();
        run("local").assert_stdout_equals(output);
    }
}
