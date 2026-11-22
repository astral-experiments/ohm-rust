//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn table(rows: usize, last: &str) {
    let prefix = r#"(&["key"], &["value"]), "#.repeat(rows - 1);
    let ranges = "(1, 'a', &['A', 'λ']), ".repeat(rows - 1);
    rfs::write(
        "table.rs",
        format!(
            r#"
pub const TABLE: &[(&[&str], &[&str])] = &[{prefix}(&[], &["", {last}])];
pub const MIXED: &[(u8, char, &[char])] = &[{ranges}(255, '🦀', &[])];
"#
        ),
    );
}

fn check(expected: &str) {
    rustc().input("table.rs").crate_type("rlib").incremental("producer").run();
    rustc().input("main.rs").extern_("table", "libtable.rlib").incremental("consumer").run();
    run("main").assert_stdout_equals(expected);
}

fn main() {
    // Cross the packing threshold, then edit a string without moving any source locations.
    table(63, r#""λ""#);
    check("63 λ");
    table(64, r#""λ""#);
    check("64 λ");
    table(64, r#""π""#);
    check("64 π");
    let thir =
        rustc().input("table.rs").crate_type("lib").arg("-Zunpretty=thir-tree").run().stdout_utf8();
    assert_eq!(thir.matches("LiteralArray(").count(), 2);

    // A non-literal in the last row must still go through ordinary const evaluation.
    table(64, r#"panic!("invalid row")"#);
    rustc()
        .input("table.rs")
        .crate_type("rlib")
        .incremental("producer")
        .run_fail()
        .assert_stderr_contains("E0080");
    table(64, r#""λ""#);
    check("64 λ");
}
