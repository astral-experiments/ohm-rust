//@ ignore-cross-compile

use run_make_support::{run, rustc};

fn merge_passes(flags: &[&str], enabled: bool) -> usize {
    let pipeline = rustc()
        .input("main.rs")
        .arg("-Ccodegen-units=1")
        .arg("-Zautodiff=PrintPasses")
        .arg(format!("-Zearly-lto-merge-functions={enabled}"))
        .args(flags)
        .run()
        .stdout_utf8();
    assert!(!pipeline.is_empty(), "missing LLVM optimization pipeline");
    pipeline.matches("mergefunc").count()
}

fn check_pipeline(flags: &[&str], extra_passes: usize, expects_merge: bool) {
    let baseline = merge_passes(flags, false);
    assert_eq!(baseline > 0, expects_merge, "{flags:?}");
    run("main");
    let enabled = merge_passes(flags, true);
    run("main");
    assert_eq!(enabled, baseline + extra_passes, "{flags:?}");
}

fn main() {
    check_pipeline(&["-Copt-level=3", "-Clto=fat"], 1, true);
    check_pipeline(&["-Copt-level=3", "-Clto=thin"], 0, true);
    check_pipeline(&["-Copt-level=3", "-Clto=off"], 0, true);
    check_pipeline(&["-Copt-level=0", "-Clto=fat"], 0, false);
    check_pipeline(&["-Copt-level=3", "-Clto=fat", "-Zmerge-functions=disabled"], 0, false);
    check_pipeline(&["-Copt-level=3", "-Clto=fat", "-Cno-prepopulate-passes"], 0, false);
}
