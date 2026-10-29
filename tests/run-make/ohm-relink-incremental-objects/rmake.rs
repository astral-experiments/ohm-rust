//@ ignore-cross-compile
// Reason: executes the resulting binary.

use std::ffi::OsStr;

use run_make_support::{rfs, run, rustc};

fn main() {
    rfs::create_dir("objects");
    for round in 0..3 {
        rustc()
            .input("main.rs")
            .incremental("incremental")
            .codegen_units(1)
            .arg("-Zemit-rlink")
            .arg("-Ztemps-dir=objects")
            .run();
        run("main").assert_stdout_equals("healthy");
        if round == 2 {
            break;
        }

        // Corrupt both freshly generated objects and objects restored on the second build.
        let mut objects = 0;
        for entry in rfs::read_dir("objects") {
            let path = entry.unwrap().path();
            if matches!(path.extension().and_then(OsStr::to_str), Some("o" | "obj")) {
                rfs::write(&path, b"bad object");
                objects += 1;
            }
        }
        assert!(objects > 0);
        rfs::remove_dir_all("objects");
        rfs::create_dir("objects");
    }
}
