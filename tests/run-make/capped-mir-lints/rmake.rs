//@ needs-target-std

use run_make_support::rustc;

fn main() {
    let capped = || {
        let mut cmd = rustc();
        cmd.input("diagnostics.rs").emit("metadata").error_format("json").arg("--cap-lints=allow");
        cmd
    };
    capped().arg("--force-warn=unused-crate-dependencies").run().assert_stderr_equals("");

    // Dead-code analysis can fulfill a lint expectation without emitting a warning.
    capped()
        .arg("--cfg=expectations")
        .arg("--force-warn=unfulfilled-lint-expectations")
        .run()
        .assert_stderr_equals("");

    // Liveness in the closure and its parent must still run if just one lint is forced.
    capped()
        .arg("--force-warn=unused-assignments")
        .run()
        .assert_stderr_contains("value assigned to `captured` is never read");
    capped()
        .arg("--force-warn=unused-variables")
        .run()
        .assert_stderr_contains("unused variable: `unused`");

    let forced = capped()
        .arg("--force-warn=unused")
        .arg("--force-warn=unconditional-recursion")
        .arg("--force-warn=const-item-mutation")
        .arg("--force-warn=function-item-references")
        .run();
    let report = capped().arg("-Zfuture-incompat-test").arg("--json=future-incompat").run();
    report.assert_stderr_contains(r#""$message_type":"future_incompat""#);

    for lint in [
        "unused_variables",
        "unused_assignments",
        "unused_imports",
        "dead_code",
        "unconditional_recursion",
        "const_item_mutation",
        "function_item_references",
    ] {
        let code = format!(r#""code":"{lint}""#);
        forced.assert_stderr_contains(&code);
        report.assert_stderr_contains(&code);
    }
}
