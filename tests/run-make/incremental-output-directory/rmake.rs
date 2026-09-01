//@ only-macos
//@ needs-target-std

// macOS does not embed CodeView object filenames or split DWARF filenames in codegen units.
// Changing the output directory must preserve backend reuse, with or without debug information.

use std::path::Path;

use run_make_support::{rfs, rust_lib_name, rustc};

fn main() {
    for debuginfo in ["0", "2"] {
        let incremental = format!("incremental-{debuginfo}");
        let initial = format!("initial-{debuginfo}");
        let relocated = format!("relocated-{debuginfo}");
        rfs::create_dir(&initial);
        rfs::create_dir(&relocated);

        let compile = |output: &str| {
            let mut compiler = rustc();
            compiler
                .input("lib.rs")
                .crate_name("relocated")
                .crate_type("rlib")
                .codegen_units(1)
                .debuginfo(debuginfo)
                .incremental(&incremental)
                .out_dir(output)
                .arg("-Zquery-dep-graph")
                .arg("-Zprint-mono-items");
            compiler
        };

        compile(&initial).run();
        compile(&relocated)
            .arg("-Zassert-incr-state=loaded")
            .run()
            .assert_stdout_contains_regex(r"(?m)^CGU_REUSE .+ PostLto$")
            .assert_stdout_not_contains_regex(r"(?m)^CGU_REUSE .+ No$");
        assert!(Path::new(&relocated).join(rust_lib_name("relocated")).is_file());
    }
}
