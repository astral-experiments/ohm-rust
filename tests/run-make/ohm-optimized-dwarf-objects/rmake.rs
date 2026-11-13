//@ only-x86_64-unknown-linux-gnu
//@ ignore-cross-compile

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use run_make_support::rustc;

fn compile(rotation: u32, split: &str) -> String {
    fs::write(
        "lib.rs",
        format!(
            "#[unsafe(no_mangle)]\npub fn value(x: u64) -> u64 {{\n\
             let y = x.rotate_left({rotation});\n\
             y.rotate_right({rotation}).wrapping_add(7)\n}}\n"
        ),
    )
    .unwrap();
    rustc()
        .input("lib.rs")
        .crate_type("rlib")
        .crate_name("fixture")
        .incremental("incremental")
        .env("CARGO_PRIMARY_PACKAGE", "1")
        .env("RUSTC_LOG", "rustc_codegen_llvm::back::dwarf_cache=info")
        .args([
            "-Zpublic-api-hash",
            "-Zohm-shared-stages",
            "-Zohm-check-relink-inputs",
            "-Copt-level=1",
            "-Ccodegen-units=1",
            "-Cembed-bitcode=no",
            "-Clto=off",
            "-Cdebuginfo=line-tables-only",
            "--emit=dep-info,metadata,link",
            "--out-dir",
            "out",
        ])
        .arg(format!("-Csplit-debuginfo={split}"))
        .run()
        .stderr_utf8()
}

fn native_files(dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            native_files(&path, files);
        } else if matches!(path.extension().and_then(|e| e.to_str()), Some("o" | "dwo")) {
            assert!(
                files.insert(path.file_name().unwrap().into(), fs::read(path).unwrap()).is_none()
            );
        }
    }
}

fn inventory() -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    native_files(Path::new("incremental"), &mut files);
    assert!(!files.is_empty());
    files
}

fn main() {
    fs::create_dir("out").unwrap();
    for split in ["unpacked", "off"] {
        let original = compile(17, split);
        let edited = compile(23, split);
        // An unpatched system LLVM also exercises the native-output comparison.
        // The two rotations differ in IR and cancel during LLVM optimization.
        if original.contains("ohm_dwarf_line_stored") {
            assert!(edited.contains("ohm_dwarf_postopt_hit"), "{edited}");
        }
        let replay = inventory();
        fs::remove_dir_all("incremental").unwrap();
        compile(23, split);
        assert!(replay == inventory(), "{split} native files differ");
        fs::remove_dir_all("incremental").unwrap();
    }
}
