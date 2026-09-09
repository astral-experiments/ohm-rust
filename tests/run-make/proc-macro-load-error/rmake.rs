//@ ignore-cross-compile
//@ only-unix
//@ needs-crate-type: proc-macro

use run_make_support::{build_native_dynamic_lib, dynamic_lib_name, rfs, run, rustc};

fn main() {
    build_native_dynamic_lib("missing_helper");
    rustc().input("macros.rs").crate_type("proc-macro").run();
    rustc()
        .input("consumer.rs")
        .edition("2021")
        .extern_("macros", dynamic_lib_name("macros"))
        .run();
    run("consumer");

    // Crate metadata is still readable, but executing the macro now requires a
    // library that the dynamic loader cannot find. Report the loader error at
    // the invocation instead of panicking while resolving the lazy client.
    rfs::remove_file(dynamic_lib_name("missing_helper"));
    rustc()
        .input("consumer.rs")
        .edition("2021")
        .extern_("macros", dynamic_lib_name("macros"))
        .run_fail()
        .assert_exit_code(1)
        .assert_stderr_contains("error:")
        .assert_stderr_contains(dynamic_lib_name("macros"))
        .assert_stderr_contains(dynamic_lib_name("missing_helper"))
        .assert_stderr_contains("consumer.rs")
        .assert_not_ice();
}
