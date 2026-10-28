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
            .incremental("incremental")
            .arg("-Zcache-proc-macros")
            .run();
        run("reader").assert_stdout_equals(format!("{value},{value}\n"));
    }
}
