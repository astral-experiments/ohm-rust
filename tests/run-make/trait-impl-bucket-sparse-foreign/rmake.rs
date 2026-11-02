//@ ignore-cross-compile
//@ needs-target-std
//@ needs-crate-type: proc-macro

use run_make_support::{dynamic_lib_name, run, rust_lib_name, rustc};

fn main() {
    // The old public function still has a DefPathHash in the proc macro, but no def-kind row.
    // Reuse the reader's old cache, then check the same edit from an empty cache.
    for (proc_mode, cache, expected) in
        [(false, "warm", "7 8"), (true, "warm", "9 8"), (true, "cold", "9 8")]
    {
        let roster = if proc_mode { dynamic_lib_name("roster") } else { rust_lib_name("roster") };
        for (name, kind) in [
            ("roster", if proc_mode { "proc-macro" } else { "rlib" }),
            ("facade", "rlib"),
            ("reader", "bin"),
        ] {
            let mut job = rustc();
            job.input(format!("{name}.rs"))
                .crate_type(kind)
                .edition("2024")
                .incremental(format!("{cache}-{name}"))
                .arg("-Cmetadata=sparse-foreign-kinds")
                .arg("-Zincremental-verify-ich");
            if name != "reader" {
                job.emit("metadata,link");
                if proc_mode {
                    job.cfg("proc_mode");
                }
            }
            if name != "roster" {
                job.arg("-Ldependency=.").extern_("roster", &roster);
            }
            if name == "reader" {
                job.extern_("facade", rust_lib_name("facade"));
            }
            job.run();
        }
        run("reader").assert_stdout_equals(expected);
    }
}
