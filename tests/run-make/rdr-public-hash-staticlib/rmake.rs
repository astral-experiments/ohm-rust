//@ ignore-cross-compile

use run_make_support::{cc, extra_c_flags, rfs, run, run_in_tmpdir, rustc, static_lib_name};

fn main() {
    for crate_types in ["staticlib", "rlib,staticlib"] {
        run_in_tmpdir(|| {
            let source = rfs::read_to_string("dep.rs");
            for value in [41, 42] {
                rfs::write("dep.rs", source.replace("41", &value.to_string()));
                rustc().input("dep.rs").crate_type("rlib").arg("-Zpublic-api-hash").run();
                rustc()
                    .input("staticlib.rs")
                    .crate_type(crate_types)
                    .extern_("dep", "libdep.rlib")
                    .arg("-Zpublic-api-hash")
                    .incremental("incremental")
                    .run();
                cc().input("main.c")
                    .input(static_lib_name("staticlib"))
                    .out_exe("main")
                    .args(extra_c_flags())
                    .run();
                run("main").assert_stdout_equals(format!("{}\n", value + 1));
            }
        });
    }
}
