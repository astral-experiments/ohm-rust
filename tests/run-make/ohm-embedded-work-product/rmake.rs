//@ only-x86_64-unknown-linux-gnu

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

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

fn certificate() -> PathBuf {
    let mut records = Vec::new();
    read_dir_entries_recursive("incremental", |p| {
        if p.extension() == Some(OsStr::new("dwarf-lines")) {
            records.push(p.to_owned());
        }
    });
    let [record] = records.try_into().unwrap_or_else(|v: Vec<PathBuf>| panic!("{v:?}"));
    record
}

fn rebuild(change: impl FnOnce(&Path)) -> CompletedProcess {
    let record = certificate();
    // Schedule the LLVM worker again while preserving this session's donor files.
    fs::remove_file(record.parent().unwrap().join("work-products.bin")).unwrap();
    change(&record);
    compile()
}

fn replace(path: &Path, bytes: &[u8]) {
    // The previous work product may still have another link in the session.
    let new = path.with_extension("replacement");
    fs::write(&new, bytes).unwrap();
    fs::rename(new, path).unwrap();
}

fn main() {
    fs::create_dir("out").unwrap();
    compile().assert_stderr_contains("ohm_dwarf_line_stored");
    let object = fs::read(certificate().with_extension("o")).unwrap();
    rebuild(|_| {}).assert_stderr_contains("exact=true");
    // Neither a different layout nor an unauthenticated compiler configuration is usable.
    for at in [0, 8, 88] {
        rebuild(|record| {
            let mut bytes = fs::read(record).unwrap();
            bytes[at] ^= 1;
            replace(record, &bytes);
        })
        .assert_stderr_not_contains("ohm_dwarf_line_hit")
        .assert_stderr_contains("ohm_dwarf_line_stored");
    }
    rebuild(|record| {
        let mut damaged = object.clone();
        damaged[0] ^= 1;
        replace(&record.with_extension("o"), &damaged);
    })
    .assert_stderr_not_contains("ohm_dwarf_line_hit")
    .assert_stderr_contains("ohm_dwarf_line_stored");
    assert_eq!(fs::read(certificate().with_extension("o")).unwrap(), object);
    rebuild(|_| {}).assert_stderr_contains("exact=true");
}
