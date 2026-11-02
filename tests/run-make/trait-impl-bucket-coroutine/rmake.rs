//@ ignore-cross-compile
//@ needs-target-std

use run_make_support::{rfs, run, rust_lib_name, rustc};

fn main() {
    let source = rfs::read_to_string("reader.rs.in");
    let shift = source.replace("// owner", "let _prior = || 0u8;");
    let remove = source.replace("async { facade::BYTE }", "facade::BYTE");
    let closure = source.replace("async { facade::BYTE }", "|| facade::BYTE");
    let signature = source.replace("fn only_coroutine()", "fn only_coroutine(_: u8)");
    let non_send = source.replace("std::sync::Arc", "std::rc::Rc");

    for (copy_impl, require_copy, text, error) in [
        (false, false, &source, None),
        (true, false, &source, None),
        (true, true, &source, None),
        (true, false, &shift, None),
        (true, false, &closure, None),
        (true, false, &remove, None),
        (
            true,
            false,
            &signature,
            Some("this function takes 1 argument"),
        ),
        (
            true,
            false,
            &non_send,
            Some("cannot be sent between threads safely"),
        ),
        (false, true, &source, Some("Toggled: Copy")),
        (false, false, &source, None),
    ] {
        let mut roster = rustc();
        roster.input("roster.rs").edition("2024").crate_type("rlib");
        if copy_impl {
            roster.cfg("copy_impl");
        }
        roster.run();
        rustc()
            .input("facade.rs")
            .edition("2024")
            .crate_type("rlib")
            .extern_("roster", rust_lib_name("roster"))
            .run();

        rfs::write("reader.rs", text);
        for (cache, next_solver) in [("default", false), ("next", true)] {
            let mut reader = rustc();
            reader
                .input("reader.rs")
                .edition("2024")
                .extern_("facade", rust_lib_name("facade"))
                .arg("-Ldependency=.")
                .incremental(cache)
                .arg("-Zincremental-verify-ich");
            if next_solver {
                reader.arg("-Znext-solver");
            }
            if require_copy {
                reader.cfg("require_copy");
            }
            if let Some(error) = error {
                reader.run_fail().assert_stderr_contains(error);
            } else {
                reader.run();
                run("reader").assert_stdout_equals("7");
            }
        }
    }
}
