//@ ignore-cross-compile
//@ only-unix
//@ ignore-aix
//@ needs-crate-type: proc-macro

use std::path::Path;

use run_make_support::{cwd, dynamic_lib_name, rfs, run, rust_lib_name, rustc};

fn main() {
    rustc().input("macros.rs").crate_type("proc-macro").run();

    derive(true);
    events("cold derive", b"LD");
    derive(true);
    events("cached derive", b"");
    verify_derived(1, 1);

    let original = rfs::read_to_string("derived.rs");
    rfs::write("derived.rs", original.replace("u8", "u16"));
    derive(true);
    events("changed derive input", b"LD");
    verify_derived(1, 2);

    // Keep the macro crate's name and path, but change its expansion result.
    rustc().input("macros.rs").crate_type("proc-macro").cfg("new_version").run();
    derive(true);
    events("changed macro binary", b"LD");
    verify_derived(2, 2);
    derive(true);
    events("cached replacement macro", b"");

    // Disabling the derive cache still executes macros on every compilation.
    for _ in 0..2 {
        derive(false);
        events("uncached derive", b"LD");
    }

    // Function-like and attribute macros must resolve their clients lazily too.
    // Neither is cached by the derive cache, including on the second compilation.
    for _ in 0..2 {
        rustc()
            .input("other.rs")
            .edition("2021")
            .extern_("macros", dynamic_lib_name("macros"))
            .incremental("other-incremental")
            .arg("-Zcache-proc-macros")
            .env("LAZY_PROC_MACRO_EVENTS", cwd().join("events"))
            .run();
        events("function-like and attribute macros", b"LBA");
        run("other");
    }
}

fn derive(cached: bool) {
    let mut command = rustc();
    command
        .input("derived.rs")
        .edition("2021")
        .crate_type("rlib")
        .extern_("macros", dynamic_lib_name("macros"))
        .env("LAZY_PROC_MACRO_EVENTS", cwd().join("events"));
    if cached {
        command.incremental("derive-incremental").arg("-Zcache-proc-macros");
    }
    command.run();
}

fn verify_derived(version: u32, size: usize) {
    rfs::write(
        "consumer.rs",
        format!(
            "fn main() {{
                assert_eq!(derived::Input::VERSION, {version});
                assert_eq!(std::mem::size_of::<derived::Input>(), {size});
            }}"
        ),
    );
    // Registering a transitive macro dependency needs its metadata, not its code.
    rustc()
        .input("consumer.rs")
        .edition("2021")
        .extern_("derived", rust_lib_name("derived"))
        .env("LAZY_PROC_MACRO_EVENTS", cwd().join("events"))
        .run();
    events("unused transitive macro dependency", b"");
    run("consumer");
}

fn events(case: &str, expected: &[u8]) {
    let mut actual = if Path::new("events").exists() {
        let contents = rfs::read("events");
        rfs::remove_file("events");
        contents
    } else {
        Vec::new()
    };
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected, "{case}");
}
