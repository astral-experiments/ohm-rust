//@ only-x86_64-unknown-linux-gnu
//@ ignore-cross-compile

use std::fs;
use std::path::{Path, PathBuf};

use run_make_support::{run, rustc};

fn compile(src: &str) -> String {
    fs::write("lib.rs", src).unwrap();
    rustc()
        .input("lib.rs")
        .edition("2024")
        .crate_type("rlib")
        .crate_name("fixture")
        .incremental("incr")
        .codegen_units(1)
        .opt_level("1")
        .debuginfo("line-tables-only")
        .env("RUSTC_LOG", "rustc_codegen_llvm::back::dwarf_cache=info")
        .args([
            "-Zpublic-api-hash",
            "-Zohm-shared-stages",
            "-Zohm-check-relink-inputs",
            "-Cembed-bitcode=no",
            "-Clto=off",
            "-Csplit-debuginfo=off",
            "--emit=dep-info,metadata,link",
        ])
        .run()
        .stderr_utf8()
}

fn find_object(dir: &Path) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if let Some(path) = find_object(&path) {
                return Some(path);
            }
        } else if path.extension().is_some_and(|ext| ext == "o") {
            return Some(path);
        }
    }
    None
}

// The normalized pre-optimizer digest follows the header and working directory.
// Keep checking it when release compilers omit tracing.
fn digest(object: &Path) -> [u8; 32] {
    let bytes = fs::read(object.with_extension("dwarf-lines")).unwrap();
    assert_eq!(&bytes[..8], b"OHMDWLN1");
    let start = 128 + u32::from_le_bytes(bytes[124..128].try_into().unwrap()) as usize;
    bytes[start..start + 32].try_into().unwrap()
}

fn check_replay(previous: &str, shifted: &str) {
    let old = compile(previous);
    let object = find_object(Path::new("incr")).unwrap();
    let before = object.with_extension("dwarf-typed").exists().then(|| digest(&object));
    let new = compile(shifted);
    let object = find_object(Path::new("incr")).unwrap();
    if let Some(before) = before {
        assert_eq!(before, digest(&object));
        if old.contains("ohm_dwarf_line_stored") {
            assert!(new.contains("ohm_dwarf_line_hit"), "{new}");
        }
    }
    let incremental = fs::read(object).unwrap();
    fs::remove_dir_all("incr").unwrap();
    compile(shifted);
    assert!(incremental == fs::read(find_object(Path::new("incr")).unwrap()).unwrap());
    fs::remove_dir_all("incr").unwrap();
}

fn main() {
    // Keep this bounds check in place. Its typed packet indicates that the
    // LLVM provider has the caller-location support needed by this test.
    let prefix = "pub fn at(xs: &[u8]) -> u8 { xs[0] }\n";
    let src = fs::read_to_string("fixture.rs").unwrap();
    check_replay(&format!("{prefix}\n{src}"), &format!("{prefix}\n\n{src}"));

    // Moving the first function across the one-byte signed LEB boundary
    // moves the following function's DW_LNE_set_address relocation.
    let funcs = "pub fn first(x: u32) -> u32 { x ^ 1 }\npub fn second(x: u32) -> u32 { x ^ 2 }\n";
    let short = format!("{prefix}{}{funcs}", "\n".repeat(30));
    let long = format!("{prefix}{}{funcs}", "\n".repeat(96));
    check_replay(&short, &long);
    check_replay(&long, &short);

    // The inlined operation and the return acquire the same numeric line
    // in different files. Reusing the object must preserve the is_stmt flags
    // that a fresh compile would emit on each file change.
    fs::write("helper.rs", "\n\n#[inline(always)]\npub fn xor(x: u32) -> u32 { x ^ 1 }\n").unwrap();
    let caller = "pub fn first(x: u32) -> u32 { helper::xor(x) }\n";
    let previous = format!("{prefix}mod helper;\n{caller}");
    let shifted = format!("{prefix}mod helper;\n\n{caller}");
    check_replay(&previous, &shifted);
    check_replay(&shifted, &previous);

    // Both calls have the same location, but the second one reads the line.
    let src = fs::read_to_string("observe.rs").unwrap();
    for src in [src.clone(), format!("\n{src}")] {
        let line = src.lines().position(|s| s.contains("both!(x)")).unwrap() + 1;
        compile(&src);
        rustc().edition("2024").input("exe.rs").extern_("fixture", "libfixture.rlib").run();
        assert_eq!(run("exe").stdout_utf8().trim(), line.to_string());
    }
}
