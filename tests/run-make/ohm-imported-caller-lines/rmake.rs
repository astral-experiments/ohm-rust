//@ only-x86_64-unknown-linux-gnu
//@ ignore-cross-compile

use std::fs;
use std::path::{Path, PathBuf};

use run_make_support::{run, rustc};

fn compile(name: &str) -> String {
    let mut command = rustc();
    if name == "facade" {
        command.extern_("supplier", "libsupplier.rlib");
    }
    command
        .input(format!("{name}.rs"))
        .edition("2024")
        .crate_type("rlib")
        .incremental(format!("{name}-incr"))
        .codegen_units(1)
        .opt_level("1")
        .debuginfo("line-tables-only")
        .env("RUSTC_LOG", "rustc_codegen_llvm::back::dwarf_cache=info")
        .args([
            "-Zpublic-api-hash",
            "-Zohm-shared-stages",
            "-Zohm-check-relink-inputs",
            "-Zremap-cwd-prefix=/sources",
            "-Cembed-bitcode=no",
            "-Clto=off",
            "-Csplit-debuginfo=off",
            "--emit=dep-info,metadata,link",
        ])
        .run()
        .stderr_utf8()
}

fn object(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).unwrap().find_map(|entry| {
        let path = entry.unwrap().path();
        if path.is_dir() {
            object(&path)
        } else {
            path.extension().is_some_and(|ext| ext == "o").then_some(path)
        }
    })
}

fn check_line(line: usize, value: u32) {
    rustc().edition("2024").input("exe.rs").extern_("facade", "libfacade.rlib").run();
    assert_eq!(run("exe").stdout_utf8().trim(), format!("({line}, {value})"));
}

fn main() {
    // A local caller takes the same two hops. If LLVM supports forwarding,
    // it can enroll the unit even when the imported source is missing.
    fs::write(
        "facade.rs",
        "pub fn call() -> (u32, u32) { supplier::origin() }\n\
         pub fn local() -> (u32, u32) { supplier::first(9) }",
    )
    .unwrap();
    fs::write(
        "exe.rs",
        r#"fn main() {
            assert_eq!(facade::local(), (2, 9));
            println!("{:?}", facade::call());
        }"#,
    )
    .unwrap();
    let source = fs::read_to_string("supplier.rs").unwrap();
    let line = source.lines().position(|row| row.contains("first(1)")).unwrap() + 1;
    let cache = Path::new("facade-incr");

    // The facade owns the location and two forwarding functions. Only the
    // opaque observer in the supplier reads the line and value at runtime.
    compile("supplier");
    let initial = compile("facade");
    check_line(line, 1);

    // A provider without the forwarding proof cannot enroll this module.
    let typed = object(cache).unwrap().with_extension("dwarf-typed").exists();
    for (index, value) in [(1, 1), (2, 7)] {
        let src = source.replace("first(1)", &format!("first({value})"));
        fs::write("supplier.rs", format!("{}{src}", "\n".repeat(index))).unwrap();
        compile("supplier");
        let log = compile("facade");
        let warm = object(cache).unwrap();
        if typed {
            // A warm miss leaves a pending record even when tracing is disabled.
            assert_eq!(warm.with_extension("dwarf-typed-pending").exists(), value != 1);
            if initial.contains("ohm_dwarf_line_stored") {
                assert_eq!(log.contains("ohm_dwarf_typed_hit"), value == 1, "{log}");
            }
        }
        check_line(line + index, value);

        // Compare the whole object, including DWARF. The fresh first edit
        // also supplies a bound donor for the second edit, which changes code.
        let bytes = fs::read(warm).unwrap();
        fs::remove_dir_all(cache).unwrap();
        compile("facade");
        assert!(bytes == fs::read(object(cache).unwrap()).unwrap());
    }
}
