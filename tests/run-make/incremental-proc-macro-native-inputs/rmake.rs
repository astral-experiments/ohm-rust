//@ only-linux
//@ ignore-cross-compile
//@ needs-crate-type: proc-macro

use run_make_support::{cc, dynamic_lib_name, rfs, run, rustc};

fn main() {
    for value in [17, 29] {
        // Relink an otherwise unchanged proc macro against new native code.
        rfs::write("value.c", format!("int value(void) {{ return {value}; }}\n"));
        cc().arg("-fPIC").arg("-c").input("value.c").output("value.o").run();
        rustc().input("native.rs").arg("-Clink-arg=value.o").run();
        rustc()
            .input("reader.rs")
            .extern_("native", dynamic_lib_name("native"))
            .incremental("legacy")
            .arg("-Zcache-proc-macros")
            .run();
        run("reader").assert_stdout_equals(format!("{value},{value}\n"));
        let compile = |list: &str| {
            let result = rustc()
                .input("reader.rs")
                .extern_("native", dynamic_lib_name("native"))
                .incremental("incremental")
                .emit("link,dep-info")
                .env("RUSTC_OHM_PURE_MACROS", list)
                .run();
            let deps = rfs::read_to_string("reader.d");
            assert!(deps.contains(&format!("# env-dep:RUSTC_OHM_PURE_MACROS={list}")));
            run("reader").assert_stdout_equals(format!("{value},{value}\n"));
            result
        };
        // Both must rerun after the native object changes. On unchanged inputs,
        // stderr lets us distinguish a hit from executing the same code again.
        let both = "native::Observe,native::observe_attr";
        compile(both)
            .assert_stderr_contains("EXECUTED derive")
            .assert_stderr_contains("EXECUTED attribute");
        compile(both)
            .assert_stderr_not_contains("EXECUTED derive")
            .assert_stderr_not_contains("EXECUTED attribute");

        let one = "native::Observe";
        compile(one)
            .assert_stderr_not_contains("EXECUTED derive")
            .assert_stderr_contains("EXECUTED attribute");
        // Restore both entries before relinking the native object.
        compile(both);
    }
}
