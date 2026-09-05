//@ ignore-cross-compile
// Reason: the compiled test binaries are executed.

use run_make_support::{cmd, run, run_with_args, rustc};

fn compile(filter: Option<&str>, output: &str) {
    let mut command = rustc();
    command.arg("--test").input("tests.rs").output(output).incremental("shared-incremental");
    if let Some(filter) = filter {
        command.arg(format!("-Ztest-harness-filter={filter}"));
    }
    command.run();
}

fn main() {
    // An ordinary test build still contains every test.
    compile(None, "unfiltered");
    run_with_args("unfiltered", &["--list"])
        .assert_stdout_contains("nested::selected: test")
        .assert_stdout_contains("nested::sibling: test")
        .assert_stdout_contains("nested::ignored: test");

    // Filtering compares complete names, not suffixes or substrings.
    compile(Some("nested::selected"), "selected");
    run_with_args("selected", &["nested::selected", "--exact"])
        .assert_stdout_contains("1 passed; 0 failed");
    run_with_args("selected", &["--exact", "nested::selected", "--no-capture"])
        .assert_stdout_contains("1 passed; 0 failed");

    // Focus validation shares libtest's ordinary parser instead of duplicating its side effects.
    let logged =
        cmd("./selected").args(["nested::selected", "--exact", "--logfile", "selected.log"]).run();
    assert_eq!(logged.stderr_utf8().matches("warning: `--logfile` is deprecated").count(), 1);

    // Invalid arguments retain libtest's actionable error instead of a generic focus error.
    cmd("./selected")
        .args(["nested::selected", "--exact", "--test-threads=0"])
        .run_fail()
        .assert_stderr_contains("argument for --test-threads must not be 0")
        .assert_stderr_not_contains("focused test harness only supports");

    // A focused binary must never silently report incomplete discovery or run an unexpected set.
    for arguments in [
        &[][..],
        &["--list"][..],
        &["--list", "--format", "terse"][..],
        &["--list", "--format", "terse", "--ignored"][..],
        &["nested::selected"][..],
        &["nested::sibling", "--exact"][..],
        &["nested::selected", "nested::sibling", "--exact"][..],
        &["nested::selected", "--exact", "--skip", "nested::sibling"][..],
        &["--help"][..],
    ] {
        cmd("./selected")
            .args(arguments)
            .run_fail()
            .assert_stderr_contains("focused test harness only supports");
    }

    // A suite containing only the requested test has no unselected descriptors to reference.
    rustc()
        .arg("--test")
        .arg("-Ztest-harness-filter=selected")
        .input("single-test.rs")
        .output("single-test")
        .run();
    run_with_args("single-test", &["selected", "--exact"])
        .assert_stdout_contains("1 passed; 0 failed");

    // Changing the tracked selection in the same incremental directory must not reuse the old
    // harness. Ignored descriptors retain their normal libtest behavior.
    compile(Some("nested::ignored"), "ignored");
    run_with_args("ignored", &["nested::ignored", "--exact"])
        .assert_stdout_contains("0 passed; 0 failed; 1 ignored");
    run_with_args("ignored", &["nested::ignored", "--exact", "--ignored"])
        .assert_stdout_contains("1 passed; 0 failed");

    // Panic-abort child processes select their test using an internal environment variable.
    rustc()
        .arg("--test")
        .arg("-Cpanic=abort")
        .arg("-Zpanic-abort-tests")
        .arg("-Ztest-harness-filter=nested::selected")
        .input("tests.rs")
        .output("panic-abort")
        .run();
    run_with_args("panic-abort", &["nested::selected", "--exact"])
        .assert_stdout_contains("1 passed; 0 failed");
    cmd("./panic-abort")
        .env("__RUST_TEST_INVOKE", "nested::sibling")
        .run_fail()
        .assert_stderr_contains("focused test harness only supports");

    // A missing or merely suffix-matching name produces an empty harness, like --exact.
    compile(Some("selected"), "missing");
    run_with_args("missing", &["selected", "--exact"]).assert_stdout_contains("running 0 tests");

    // Coverage must retain all tests and counters even when an exact selection was requested.
    rustc()
        .arg("--test")
        .arg("-Cinstrument-coverage")
        .arg("-Zno-profiler-runtime")
        .arg("-Ztest-harness-filter=nested::selected")
        .input("tests.rs")
        .output("coverage")
        .run();
    run_with_args("coverage", &["--list"])
        .assert_stdout_contains("nested::selected: test")
        .assert_stdout_contains("nested::sibling: test");

    // Removing the tracked option restores the complete harness in the same incremental cache.
    compile(None, "restored");
    run_with_args("restored", &["--list"])
        .assert_stdout_contains("nested::selected: test")
        .assert_stdout_contains("nested::sibling: test");

    // Unselected bodies remain in the crate and are fully typechecked.
    rustc()
        .arg("--test")
        .arg("-Ztest-harness-filter=selected")
        .input("invalid-unselected.rs")
        .run_fail()
        .assert_stderr_contains("mismatched types");

    // Lints still inspect unselected bodies even though their code is not rooted.
    rustc()
        .arg("--test")
        .arg("-Ztest-harness-filter=selected")
        .input("lint-unselected.rs")
        .run_fail()
        .assert_stderr_contains("unused variable: `must_still_lint`");

    // The explicit opt-in does intentionally omit backend/link-only diagnostics in unreachable
    // tests. In particular, pruning an unselected test can remove an otherwise unresolved symbol.
    rustc()
        .arg("--test")
        .input("missing-symbol.rs")
        .output("missing-symbol-unfiltered")
        .run_fail()
        .assert_stderr_contains("missing_only_from_unselected_test");
    rustc()
        .arg("--test")
        .arg("-Ztest-harness-filter=selected")
        .input("missing-symbol.rs")
        .output("missing-symbol-filtered")
        .run();
    run_with_args("missing-symbol-filtered", &["selected", "--exact"])
        .assert_stdout_contains("1 passed; 0 failed");

    // Custom runners can inspect the complete test list and must never be narrowed.
    rustc()
        .arg("--test")
        .arg("-Ztest-harness-filter=selected")
        .input("custom-runner.rs")
        .output("custom-runner")
        .run();
    run("custom-runner").assert_stdout_contains("2 custom tests");

    // Reexported entry points can likewise be called directly and must retain every test.
    rustc()
        .arg("--test")
        .arg("-Ztest-harness-filter=selected")
        .input("reexported-runner.rs")
        .output("reexported-runner")
        .run();
    run_with_args("reexported-runner", &["--list"])
        .assert_stdout_contains("selected: test")
        .assert_stdout_contains("unselected: test");

    // A custom test-case marker can differ from its actual runtime descriptor name even without
    // a custom runner. Preserve the complete harness whenever that feature is enabled.
    rustc()
        .arg("--test")
        .arg("-Ztest-harness-filter=actual")
        .input("custom-descriptor.rs")
        .output("custom-descriptor")
        .run();
    run_with_args("custom-descriptor", &["--list"]).assert_stdout_contains("actual: test");
    run_with_args("custom-descriptor", &["actual", "--exact"])
        .assert_stdout_contains("1 passed; 0 failed");
}
