use run_make_support::{rustc, source_root};

fn main() {
    let chained_constants =
        source_root().join("tests/ui/consts/chained-constants-stackoverflow.rs");

    // Parallel evaluation may discover this upstream regression's 350 constants
    // in a different order, requiring a deeper query stack than sequential evaluation.
    rustc().input(&chained_constants).arg("-Zthreads=8").output("chained-parallel").run();

    // The thread count is untracked, so changing modes must not change the
    // stable fingerprint of the cached compiler-limits query.
    for threads in ["1", "8", "1", "sync"] {
        rustc()
            .input(&chained_constants)
            .arg("-Zunstable-options")
            .arg(format!("-Zthreads={threads}"))
            .incremental("shared-incremental")
            .output("chained-incremental")
            .run();
    }

    // An explicit crate limit still controls this deterministic nested-layout query.
    rustc()
        .input(source_root().join("tests/ui/query-system/query_depth.rs"))
        .arg("-Zthreads=8")
        .output("explicit-low")
        .run_fail()
        .assert_stderr_contains("queries overflow the depth limit!")
        .assert_stderr_contains("recursion_limit = \"128\"");

    rustc()
        .input(&chained_constants)
        .arg("-Zthreads=8")
        .arg(r#"-Zcrate-attr=recursion_limit="512""#)
        .output("explicit-high")
        .run();

    // The unstable minimum remains authoritative for a genuinely unbounded query
    // instead of receiving the automatic parallel-only allowance.
    rustc()
        .input(source_root().join("tests/ui/consts/recursive-block.rs"))
        .arg("-Zthreads=8")
        .arg("-Zmin-recursion-limit=256")
        .output("explicit-min-low")
        .run_fail()
        .assert_stderr_contains("queries overflow the depth limit!")
        .assert_stderr_contains("recursion_limit = \"512\"");

    rustc()
        .input(&chained_constants)
        .arg("-Zthreads=8")
        .arg("-Zmin-recursion-limit=512")
        .output("explicit-min-high")
        .run();

    // The larger default query allowance remains bounded, with an actionable
    // suggestion based on its effective limit rather than the ordinary limit.
    rustc()
        .input(source_root().join("tests/ui/consts/recursive-block.rs"))
        .arg("-Zthreads=8")
        .output("unbounded-query")
        .run_fail()
        .assert_stderr_contains("queries overflow the depth limit!")
        .assert_stderr_contains("recursion_limit = \"1024\"");

    // Recursive query cycles remain errors instead of being hidden by the allowance.
    rustc()
        .input(source_root().join("tests/ui/consts/const-size_of-cycle.rs"))
        .arg("-Zthreads=8")
        .output("recursive-cycle")
        .run_fail()
        .assert_stderr_contains("cycle detected");

    // Macro expansion retains the original default of 128, as demonstrated by
    // its suggestion to increase the crate recursion limit to 256.
    rustc()
        .input("recursive-macro.rs")
        .arg("-Zthreads=8")
        .output("recursive-macro")
        .run_fail()
        .assert_stderr_contains("recursion limit reached")
        .assert_stderr_contains("recursion_limit = \"256\"");
}
