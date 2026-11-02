//@ ignore-cross-compile
//@ needs-target-std

use run_make_support::{rfs, run, rust_lib_name, rustc};

fn main() {
    let roster = rfs::read_to_string("roster.rs");
    let reader = rfs::read_to_string("reader.rs.in");
    let signature = roster.replace("fn seed()", "fn seed(_: u8)");
    let constructor = roster.replace("Make(u8)", "Make(u8, u8)");
    let method = roster.replace("fn value()", "fn value(_: u8)");
    let changed = reader.replace("let f = facade::seed", "let f = facade::replacement");
    let noncopy = reader.replace("let _saved = 7u8", "let _saved = String::from(\"seven\")");
    let replaced = reader.replace("move || { let _ = &_saved; 6 }", "local");
    let shifted = reader.replace("let chosen = ", "let _prior = || 0;\n    let chosen = ");
    let retired = roster.replace("pub fn seed() -> u8 { 7 }", "pub const seed: u8 = 7;");
    let normal = Ok("7 7 5 6 4");

    for (toggle, owner, caller, expected) in [
        (false, &roster, &reader, normal),
        (true, &roster, &reader, normal),
        (true, &signature, &reader, Err("E0061")),
        (true, &constructor, &reader, Err("E0061")),
        (true, &method, &reader, Err("E0061")),
        (true, &retired, &reader, Err("E0618")),
        (true, &roster, &changed, Ok("9 7 5 6 4")),
        (true, &roster, &noncopy, Err("E0277")),
        (true, &roster, &replaced, Ok("7 7 5 4 4")),
        (true, &roster, &shifted, normal),
        (false, &roster, &reader, normal),
    ] {
        rfs::write("current.rs", owner);
        let mut library = rustc();
        library
            .input("current.rs")
            .crate_name("roster")
            .crate_type("rlib")
            .edition("2024");
        if toggle {
            library.cfg("copy_impl");
        }
        library.run();
        rustc()
            .input("facade.rs")
            .crate_type("rlib")
            .edition("2024")
            .extern_("roster", rust_lib_name("roster"))
            .run();

        rfs::write("reader.rs", caller);
        for (cache, next) in [("ordinary", false), ("next", true)] {
            let mut job = rustc();
            job.input("reader.rs")
                .edition("2024")
                .extern_("facade", rust_lib_name("facade"))
                .arg("-Ldependency=.")
                .incremental(cache)
                .arg("-Zincremental-verify-ich");
            if next {
                job.arg("-Znext-solver");
            }
            match expected {
                Ok(stdout) => {
                    job.run();
                    run("reader").assert_stdout_equals(stdout);
                }
                Err(code) => {
                    job.run_fail().assert_stderr_contains(code);
                }
            }
        }
    }
}
