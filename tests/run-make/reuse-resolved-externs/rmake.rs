//@ ignore-cross-compile

// Repeated direct imports must preserve command-line renames, distinct transitive
// versions, no_link upgrades, proc-macro imports, and generated libtest imports.

use run_make_support::{bin_name, dynamic_lib_name, run, rust_lib_name, rustc};

fn main() {
    for (version, suffix) in [("first", "-first"), ("second", "-second")] {
        rustc()
            .input("dep.rs")
            .crate_name("dep")
            .crate_type("rlib")
            .cfg(version)
            .metadata(version)
            .extra_filename(suffix)
            .run();
    }
    rustc().input("bridge.rs").crate_type("rlib").extern_("dep", rust_lib_name("dep-first")).run();
    rustc().input("macros.rs").crate_type("proc-macro").run();

    for (output, flags) in [
        ("checked.rmeta".to_owned(), vec!["--emit=metadata"]),
        (bin_name("main"), vec![]),
        (bin_name("test-harness"), vec!["--test"]),
    ] {
        let mut compiler = rustc();
        compiler
            .input("main.rs")
            .library_search_path(".")
            .extern_("bridge", rust_lib_name("bridge"))
            .extern_("dep", rust_lib_name("dep-second"))
            .extern_("chosen", rust_lib_name("dep-first"))
            .extern_("macros", dynamic_lib_name("macros"))
            .output(output);
        for flag in flags {
            compiler.arg(flag);
        }
        compiler.run();
    }

    run("main");
    run("test-harness").assert_stdout_contains("3 passed; 0 failed");
}
