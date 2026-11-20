//@ only-x86_64-unknown-linux-gnu

use std::fs;

use run_make_support::{CompletedProcess, run, rustc};

fn compile_reader(extra: &[&str]) -> CompletedProcess {
    compile_reader_in("out", extra)
}

fn compile_reader_in(output: &str, extra: &[&str]) -> CompletedProcess {
    rustc()
        .input("reader.rs")
        .crate_type("rlib")
        .extern_("foreign", "libforeign.rlib")
        .extern_("observer", "libobserver.so")
        .args([
            "-Zpublic-api-hash",
            "-Zohm-shared-stages",
            "-Zohm-check-relink-inputs",
            "-Copt-level=1",
            "-Ccodegen-units=1",
            "-Clto=off",
            "--emit=metadata,link,dep-info",
            "--out-dir",
            output,
        ])
        .args(extra)
        .env(
            "RUSTC_LOG",
            "rustc_codegen_llvm::back=info,rustc_codegen_ssa::back::loaded_libraries=info",
        )
        .run()
}

fn expect_value(value: &str) {
    rustc().input("main.rs").extern_("reader", "out/libreader.rlib").arg("-Ldependency=.").run();
    run("main").assert_stdout_equals(value);
}

fn main() {
    fs::create_dir("out").unwrap();
    rustc().input("foreign.rs").crate_type("rlib").arg("-Zpublic-api-hash").run();
    rustc().input("observer.rs").crate_type("proc-macro").run();
    fs::write("number", "17").unwrap();
    compile_reader(&[]).assert_stderr_contains("ohm_observer_object_miss");
    assert!(fs::read_to_string("out/reader.d").unwrap().contains("ohm-foreign-span-bytes"));
    let archive = fs::read("out/libreader.rlib").unwrap();
    compile_reader(&[]).assert_stderr_contains("ohm_observer_object_hit");
    compile_reader(&["-Ztime-llvm-passes"]).assert_stderr_not_contains("ohm_observer_object_");
    compile_reader(&["-Zprint-codegen-stats-json=llvm-stats.json"])
        .assert_stderr_not_contains("ohm_observer_object_");
    assert_eq!(fs::read("out/libreader.rlib").unwrap(), archive);
    expect_value("17 0");

    fs::write("number", "29").unwrap();
    compile_reader(&[]).assert_stderr_contains("ohm_observer_object_miss");
    expect_value("29 0");
    compile_reader(&[]).assert_stderr_contains("ohm_observer_object_hit");

    let mut slots = fs::read_dir("out/.ohm-observer-objects").unwrap();
    let slot = slots.next().unwrap().unwrap().path();
    assert!(slots.next().is_none());
    fs::write(slot, b"damaged object").unwrap();
    compile_reader(&[]).assert_stderr_contains("ohm_observer_object_miss");
    expect_value("29 0");

    fs::write("payload", [1]).unwrap();
    compile_reader(&["--cfg", "asm_input"]).assert_stderr_not_contains("ohm_observer_object_hit");
    expect_value("29 1");
    fs::write("payload", [2]).unwrap();
    compile_reader(&["--cfg", "asm_input"]).assert_stderr_not_contains("ohm_observer_object_hit");
    expect_value("29 2");

    // This small module needs full debuginfo for LLVM to emit an identifiable split unit.
    let split = ["-Cdebuginfo=2", "-Csplit-debuginfo=unpacked"];
    let first = compile_reader(&split);
    let archive = fs::read("out/libreader.rlib").unwrap();
    if first.stderr_utf8().contains("ohm_split_observer_llvm_unavailable") {
        first.assert_stderr_not_contains("ohm_observer_object_hit");
        compile_reader(&split)
            .assert_stderr_contains("ohm_split_observer_llvm_unavailable")
            .assert_stderr_not_contains("ohm_observer_object_hit");
        assert_eq!(fs::read("out/libreader.rlib").unwrap(), archive);
        expect_value("29 0");
        return;
    }
    first.assert_stderr_contains("ohm_observer_object_miss");
    let dwarf = fs::read_dir("out")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "dwo"))
        .unwrap();
    let bytes = fs::read(&dwarf).unwrap();
    fs::remove_file(&dwarf).unwrap();
    compile_reader(&split).assert_stderr_contains("ohm_observer_object_hit");
    assert_eq!(fs::read(&dwarf).unwrap(), bytes);
    assert_eq!(fs::read("out/libreader.rlib").unwrap(), archive);

    let slot = fs::read_dir("out/.ohm-observer-objects").unwrap().next().unwrap().unwrap().path();
    let mut invalid = fs::read(&slot).unwrap();
    *invalid.last_mut().unwrap() ^= 1;
    fs::write(&slot, invalid).unwrap();
    compile_reader(&split).assert_stderr_contains("ohm_observer_object_miss");
    assert_eq!(fs::read("out/libreader.rlib").unwrap(), archive);
    fs::create_dir_all("other/.ohm-observer-objects").unwrap();
    fs::copy(
        &slot,
        std::path::Path::new("other/.ohm-observer-objects").join(slot.file_name().unwrap()),
    )
    .unwrap();
    compile_reader_in("other", &split).assert_stderr_contains("ohm_observer_object_miss");
    fs::write("number", "41").unwrap();
    compile_reader(&split).assert_stderr_contains("ohm_observer_object_miss");
    expect_value("41 0");
}
