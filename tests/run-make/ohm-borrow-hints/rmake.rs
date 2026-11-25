//@ only-x86_64-unknown-linux-gnu

use run_make_support::{rfs, run, rustc};

fn compile(name: &str, relink: bool) -> String {
    let mut cmd = rustc();
    cmd.input(format!("{name}.rs"))
        .edition("2024")
        .args(["-Zpublic-api-hash", "-Zincremental-verify-ich", "-Clto=off"])
        .opt_level("1")
        .debuginfo("line-tables-only")
        .codegen_units(1)
        .incremental("incr")
        .emit("link,llvm-ir");
    if relink {
        cmd.arg("-Zohm-check-relink-inputs");
    }
    if name == "up" {
        cmd.crate_type("rlib");
    } else {
        cmd.extern_("up", "libup.rlib");
    }
    cmd.run();
    rfs::read_to_string(format!("{name}.ll"))
}

fn check(ir: &str, op: &str, name: &str, expected: &[usize]) {
    let line = ir.lines().find(|l| l.contains(op) && l.contains(name)).unwrap();
    let sizes: Vec<usize> = line
        .split("dereferenceable(")
        .skip(1)
        .map(|s| s.split_once(')').unwrap().0.parse().unwrap())
        .collect();
    assert_eq!(sizes, expected, "{line}");
}

fn main() {
    // Move the private tail within one size bucket, cross a boundary, then shrink.
    for (size, hint) in [(63, 63), (72, 64), (76, 64), (80, 80), (81, 80), (72, 64)] {
        rfs::write("length.rs", (size - 1).to_string());
        let up = compile("up", true);
        let down = compile("down", true);
        check(&up, "define ", "@compare_blocks(", &[size, size]);
        check(&down, "declare ", "@compare_blocks(", &[hint, hint]);
        check(&down, "call ", "@compare_blocks(", &[hint, hint]);
        for op in ["declare ", "call "] {
            check(&down, op, "@c_tail(", &[size]);
            check(&down, op, "@unrounded(", &[size, size, size]);
        }
        // The downstream definition needs the full guarantee to speculate a tail load.
        check(&down, "define ", "@select_tail(", &[size, size]);
        assert_eq!(run("down").stdout_utf8().trim(), size.to_string());
    }
    let down = compile("down", false);
    check(&down, "declare ", "@compare_blocks(", &[72, 72]);
    check(&down, "call ", "@compare_blocks(", &[72, 72]);
}
