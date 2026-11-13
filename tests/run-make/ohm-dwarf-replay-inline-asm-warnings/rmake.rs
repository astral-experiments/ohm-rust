//@ only-x86_64-unknown-linux-gnu

use run_make_support::{rfs, rustc};

fn main() {
    let source = rfs::read_to_string("warning.rs");
    let warning_line = source.lines().position(|line| line.contains("asm!")).unwrap() + 1;
    rfs::create_dir("out");
    for split in ["off", "unpacked"] {
        for blank_lines in [0, 3, 0, 2] {
            rfs::write("lib.rs", format!("{}{source}", "\n".repeat(blank_lines)));

            // The assembler reports this warning after optimization. Reusing an older
            // object for this module could suppress it or point to a previous line.
            rustc()
                .input("lib.rs")
                .crate_type("rlib")
                .incremental(format!("incremental-{split}"))
                .codegen_units(1)
                .opt_level("1")
                .arg(format!("-Csplit-debuginfo={split}"))
                .args([
                    "-Cdebuginfo=line-tables-only",
                    "-Clto=off",
                    "-Cembed-bitcode=no",
                    "-Zohm-check-relink-inputs",
                    "--emit=dep-info,metadata,link",
                    "--out-dir",
                    "out",
                ])
                .env("CARGO_PRIMARY_PACKAGE", "1")
                .run()
                .assert_stderr_contains("warning: Ohm object cache warning")
                .assert_stderr_contains(format!("lib.rs:{}:", warning_line + blank_lines))
                .assert_stderr_contains("instantiated into assembly here");
        }
    }
}
