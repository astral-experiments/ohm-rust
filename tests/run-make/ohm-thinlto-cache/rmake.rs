//@ only-x86_64-unknown-linux-gnu

use std::fs;
use std::path::PathBuf;

use run_make_support::{CompletedProcess, cmd, run, rustc};

fn build(cache: Option<&str>, extra: &[&str]) -> CompletedProcess {
    let mut command = rustc();
    command
        .input("main.rs")
        .extern_("dep", "libdep.rlib")
        .opt_level("3")
        .lto("thin")
        .arg("-Ccodegen-units=2")
        .env(
            "RUSTC_LOG",
            "rustc_codegen_llvm::back::lto=info,rustc_codegen_ssa::back::loaded_libraries=info",
        );
    if let Some(cache) = cache {
        command.arg(format!("-Zthinlto-cache={cache}"));
    }
    command.args(extra).run()
}

fn entries(root: &str) -> Vec<PathBuf> {
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .flat_map(|dir| fs::read_dir(dir.unwrap().path()).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "cache"))
        .collect()
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("fifo") {
        build(Some("cache"), &[]).assert_stderr_contains("ohm_thinlto_cache_miss");
        return;
    }

    rustc().input("dep.rs").crate_type("rlib").opt_level("3").run();
    build(None, &[]);
    let baseline = fs::read("main").unwrap();
    build(Some("cache"), &[]);
    build(Some("cache"), &[]).assert_stderr_contains("ohm_thinlto_cache_hit");
    assert_eq!(baseline, fs::read("main").unwrap());
    run("main").assert_stdout_equals("42\n");

    let entry = entries("cache").pop().unwrap();
    fs::remove_file(&entry).unwrap();
    cmd("mkfifo").arg(&entry).run();
    cmd("timeout").arg("60").arg(std::env::current_exe().unwrap()).arg("fifo").run();
    assert!(entry.is_file());
    assert_eq!(baseline, fs::read("main").unwrap());

    for entry in entries("cache") {
        fs::write(entry, "corrupt").unwrap();
    }
    build(Some("cache"), &[]).assert_stderr_not_contains("ohm_thinlto_cache_hit");
    assert_eq!(baseline, fs::read("main").unwrap());

    // A changed imported body must invalidate its consumer even though the
    // consumer's source, compiler options, and dependency path are unchanged.
    fs::write("dep.rs", "#[inline(always)] pub fn answer() -> u32 { 43 }").unwrap();
    rustc().input("dep.rs").crate_type("rlib").opt_level("3").run();
    build(Some("cache"), &[]);
    let changed = fs::read("main").unwrap();
    run("main").assert_stdout_equals("43\n");
    build(None, &[]);
    assert_eq!(changed, fs::read("main").unwrap());

    build(Some("cache"), &["-Copt-level=2"]).assert_stderr_not_contains("ohm_thinlto_cache_hit");
    run("main").assert_stdout_equals("43\n");
    fs::write("blocked-cache", "not a directory").unwrap();
    build(Some("blocked-cache"), &[]);
    assert_eq!(changed, fs::read("main").unwrap());

    build(Some("incremental-cache"), &["-Cincremental=incremental"]);
    build(Some("extra-output-cache"), &["--emit=link,llvm-ir"]);
    assert!(entries("incremental-cache").is_empty());
    assert!(entries("extra-output-cache").is_empty());

    // Fat-LTO bitcode can omit the content hash used in LLVM's cache key.
    rustc().input("dep.rs").crate_type("rlib").opt_level("3").lto("fat").run();
    build(Some("hashless-cache"), &[]);
    assert!(entries("hashless-cache").is_empty());
    run("main").assert_stdout_equals("43\n");

    // Global assembly can consume files absent from the bitcode hash. Changing
    // such a file must affect the executable on a warm build.
    fs::write(
        "dep.rs",
        r#"
        core::arch::global_asm!(
            ".warning \"cached asm warning\"",
            ".pushsection .rodata",
            ".global answer_data",
            "answer_data:",
            ".incbin \"answer.bin\"",
            ".popsection"
        );
        unsafe extern "C" { static answer_data: u8; }
        pub fn answer() -> u32 { unsafe { answer_data as u32 } }
        "#,
    )
    .unwrap();
    fs::write("answer.bin", [44]).unwrap();
    rustc().input("dep.rs").crate_type("rlib").opt_level("3").run();
    build(Some("asm-cache"), &[]).assert_stderr_contains("cached asm warning");
    run("main").assert_stdout_equals("44\n");
    fs::write("answer.bin", [45]).unwrap();
    build(Some("asm-cache"), &[]).assert_stderr_contains("cached asm warning");
    run("main").assert_stdout_equals("45\n");
}
