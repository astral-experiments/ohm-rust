//@ only-x86_64-unknown-linux-gnu

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use run_make_support::path_helpers::read_dir_entries_recursive;
use run_make_support::{CompletedProcess, rustc};

fn compile() -> CompletedProcess {
    rustc()
        .input("lib.rs")
        .crate_name("witness")
        .crate_type("rlib")
        .incremental("incremental")
        .codegen_units(1)
        .args([
            "--emit=dep-info,metadata,link",
            "-Copt-level=1",
            "-Clto=off",
            "-Cdebuginfo=line-tables-only",
            "-Csplit-debuginfo=off",
            "-Cembed-bitcode=no",
            "-Zohm-check-relink-inputs",
            "--out-dir=out",
        ])
        .env("RUSTC_LOG", "rustc_codegen_llvm::back::dwarf_cache=info")
        .run()
}

fn record() -> PathBuf {
    let mut matches = Vec::new();
    read_dir_entries_recursive("incremental", |path| {
        if path.extension() == Some(OsStr::new("dwarf-lines")) {
            matches.push(path.to_owned());
        }
    });
    let [file] = matches.try_into().unwrap_or_else(|paths: Vec<PathBuf>| panic!("{paths:?}"));
    file
}

fn main() {
    fs::create_dir("out").unwrap();
    // Give the first DWARF row an explicit advance_line operand that can be updated in place.
    let source = format!("{}{}", "\n".repeat(20), fs::read_to_string("input.rs").unwrap());
    fs::write("lib.rs", &source).unwrap();
    compile().assert_stderr_contains("ohm_dwarf_line_stored");

    fs::write("lib.rs", format!("\n{source}")).unwrap();
    fs::remove_file(record().parent().unwrap().join("work-products.bin")).unwrap();
    compile().assert_stderr_contains("exact=false");
    let donor = record();
    let replayed = fs::read(donor.with_extension("o")).unwrap();
    fs::remove_file(donor.parent().unwrap().join("work-products.bin")).unwrap();
    fs::remove_file(donor).unwrap();
    compile().assert_stderr_not_contains("ohm_dwarf_line_hit");
    assert_eq!(fs::read(record().with_extension("o")).unwrap(), replayed);

    let warning =
        source.replace("/* {value} */", r#"/* {value} */\n.warning \"register comment warning\""#);
    for blanks in [0, 3] {
        fs::write("lib.rs", format!("{}{warning}", "\n".repeat(blanks))).unwrap();
        let line = warning.lines().position(|s| s.contains("asm!")).unwrap() + 1 + blanks;
        compile()
            .assert_stderr_not_contains("ohm_dwarf_line_hit")
            .assert_stderr_contains("warning: register comment warning")
            .assert_stderr_contains(format!("lib.rs:{line}:"));
    }
}
