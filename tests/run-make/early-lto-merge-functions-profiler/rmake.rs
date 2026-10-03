//@ ignore-cross-compile
//@ needs-profiler-runtime

use run_make_support::{run, rustc};

fn pipeline(flags: &[&str], enabled: bool) -> String {
    let pipeline = rustc()
        .input("main.rs")
        .arg("-Ccodegen-units=1")
        .arg("-Zautodiff=PrintPasses")
        .arg(format!("-Zearly-lto-merge-functions={enabled}"))
        .args(flags)
        .run()
        .stdout_utf8();
    assert!(!pipeline.is_empty(), "missing LLVM optimization pipeline");
    pipeline
}

fn check_pipeline(flags: &[&str]) {
    let baseline = pipeline(flags, false);
    run("main");
    let enabled = pipeline(flags, true);
    run("main");
    assert_eq!(enabled, baseline, "{flags:?}");
}

fn main() {
    check_pipeline(&["-Copt-level=3", "-Clto=fat", "-Cprofile-generate=pgo"]);
    check_pipeline(&["-Copt-level=3", "-Clto=fat", "-Cinstrument-coverage"]);
}
