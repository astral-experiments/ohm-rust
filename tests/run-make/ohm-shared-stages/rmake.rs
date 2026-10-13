//@ ignore-cross-compile
// Reason: the test runs the built consumer after each stage transition.

use std::fs;
use std::path::PathBuf;

use run_make_support::{rfs, run, rustc};

fn source(delta: u64, width: &str, checked_width: &str) {
    rfs::write(
        "dep.rs",
        r#"
#![feature(rustc_attrs)]
#![allow(internal_features)]
#![rustc_partition_reused(module = "stage_dep-untouched", cfg = "latest_check")]

pub mod untouched {
    #[inline(never)]
    pub fn value() -> u64 { 3 }
}

#[rustc_clean(cfg = "from_check")]
#[rustc_clean(cfg = "check_again")]
#[rustc_clean(cfg = "warm_build")]
pub const BASE: u64 = 41;

pub fn width() -> WIDTH { 0 }
fn measured_width<T>(_: T) -> u64 { std::mem::size_of::<T>() as u64 }
#[rustc_clean(
    cfg = "latest_check",
    except = "hir_owner,optimized_mir",
    loaded_from_disk = "typeck_root,mir_borrowck"
)]
pub fn value() -> u64 { BASE + DELTA + measured_width(width()) }
pub fn generic<T>(value: T) -> T { value }
pub fn opaque() -> impl Fn() -> u64 { || value() }
pub async fn future() -> u64 { value() }

#[cfg(check_only)]
pub fn checked_signature() { let _: CHECKED_WIDTH = width(); }
"#
        .replace("CHECKED_WIDTH", checked_width)
        .replace("WIDTH", width)
        .replace("DELTA", &delta.to_string()),
    );
}

fn compile(emit: &str, revision: &str, incremental: bool, success: bool) {
    let mut command = rustc();
    command
        .input("dep.rs")
        .crate_name("stage_dep")
        .crate_type("rlib")
        .edition("2024")
        .emit(emit)
        .cfg(revision)
        .codegen_units(16)
        .arg("-Zohm-shared-stages")
        .arg("-Zquery-dep-graph")
        .arg("-Zincremental-verify-ich");
    if incremental {
        command.incremental("incremental");
    }
    if emit == "metadata" && revision == "check_signature" {
        command.cfg("check_only");
    }
    if success {
        command.run();
    } else {
        command.run_fail();
    }
}

fn latest_session() -> PathBuf {
    let crate_dir = fs::read_dir("incremental")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.is_dir())
        .unwrap();
    fs::read_dir(crate_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir() && !path.to_string_lossy().ends_with("-working"))
        .max()
        .unwrap()
}

fn assert_native_checkpoint() {
    let session = latest_session();
    assert!(session.join("ohm-build-dep-graph.bin").is_file());
    assert!(session.join("ohm-build-query-cache.bin").is_file());
    assert!(fs::read_dir(session).unwrap().any(|entry| {
        entry.unwrap().path().extension().is_some_and(|extension| extension == "o")
    }));
}

fn consumer(expected: u64) {
    rustc().input("main.rs").edition("2024").extern_("stage_dep", "libstage_dep.rlib").run();
    run("main").assert_stdout_equals(format!("{expected}\n"));
}

fn main() {
    rfs::write(
        "main.rs",
        r#"
fn main() {
    fn future(_: impl std::future::Future<Output = u64>) {}
    future(stage_dep::future());
    println!("{}", stage_dep::generic(stage_dep::opaque()()));
}
"#,
    );

    source(1, "u64", "u64");
    compile("metadata", "initial", true, true);
    assert!(!std::path::Path::new("libstage_dep.rlib").exists());
    compile("metadata,link", "from_check", true, true);
    assert_native_checkpoint();
    consumer(50);

    source(2, "u64", "u64");
    compile("metadata", "check_again", true, true);
    assert_native_checkpoint();
    compile("metadata,link", "latest_check", true, true);
    consumer(51);

    compile("metadata", "check_again", true, true);
    source(2, "bool", "bool");
    compile("metadata,link", "unchecked_error", true, false);
    source(3, "u64", "u64");
    compile("metadata,link", "unchecked_edit", true, true);
    consumer(52);

    source(2, "u32", "u64");
    compile("metadata,link", "warm_build", true, true);
    consumer(47);
    compile("metadata", "check_signature", true, false);

    source(2, "u32", "u32");
    compile("metadata", "check_signature", true, true);
    assert_native_checkpoint();
    compile("metadata,link", "warm_build", true, true);
    consumer(47);

    compile("metadata,link", "fresh", false, true);
    consumer(47);

    rustc()
        .input("dep.rs")
        .crate_name("stage_dep")
        .crate_type("rlib")
        .edition("2024")
        .emit("llvm-ir")
        .incremental("incremental")
        .codegen_units(2)
        .arg("-Zohm-shared-stages")
        .arg("-Zassert-incr-state=not-loaded")
        .arg("-Zquery-dep-graph")
        .arg("-Zincremental-verify-ich")
        .run();
    compile("metadata,link", "other_output", true, true);
    consumer(47);
}
