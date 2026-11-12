//@ only-x86_64
//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn check(name: &str, args: &[&str], inlined: bool, untracked_inputs: bool) {
    rfs::create_dir(name);
    let mut cmd = rustc();
    cmd.input("fixture.rs")
        .arg("--edition=2021")
        .arg("--crate-name=inlining")
        .arg("--emit=llvm-ir,dep-info,link")
        .arg("-Ccodegen-units=1")
        .arg("-Copt-level=1")
        .arg("-Zohm-check-relink-inputs")
        .arg("--out-dir")
        .arg(name);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.run();

    let ir = rfs::read_to_string(format!("{name}/inlining.ll"));
    let caller = ir
        .lines()
        .skip_while(|line| !(line.starts_with("define ") && line.contains("@caller(")))
        .take_while(|line| *line != "}")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(caller.contains("@caller("), "{caller}");
    assert_eq!(caller.contains("@step("), inlined, "{caller}");
    assert_eq!(caller.contains("hinted"), !inlined, "{caller}");
    let deps = rfs::read_to_string(format!("{name}/inlining.d"));
    assert_eq!(deps.contains("ohm-untracked-relink-inputs"), untracked_inputs, "{deps}");
}

fn main() {
    check("library", &["--crate-type=lib"], true, false);
    check("harness", &["--test"], false, false);
    check("higher", &["--test", "-Copt-level=2"], true, false);
    check("custom", &["--test", "-Cllvm-args=-inlinehint-threshold=325"], true, true);
    run("harness/inlining");
}
