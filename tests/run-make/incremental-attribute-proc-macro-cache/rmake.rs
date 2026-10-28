//@ ignore-cross-compile
//@ needs-crate-type: proc-macro
//@ ignore-musl (FIXME: can't find `-lunwind`)

use run_make_support::{Rustc, rfs, run, rustc};

fn compile(input: &str, strategy: &str) -> Rustc {
    let mut cmd = rustc();
    cmd.input(format!("{input}.rs"))
        .incremental(format!("incremental-{input}-{strategy}"))
        .arg("-Zcache-proc-macros")
        .arg(format!("-Zproc-macro-execution-strategy={strategy}"))
        .emit("link,dep-info");
    cmd
}

fn main() {
    let macro_source = rfs::read_to_string("attrs.rs");
    let pure_source = rfs::read_to_string("pure.rs");
    let observation_source = rfs::read_to_string("observations.rs");

    for strategy in ["same-thread", "cross-thread"] {
        rfs::write("attrs.rs", &macro_source);
        rfs::write("pure.rs", &pure_source);
        rustc().input("attrs.rs").run();

        compile("pure", strategy)
            .run()
            .assert_stderr_contains("EXECUTED cached attribute")
            .assert_stderr_contains("cached attribute warning")
            .assert_stderr_contains("pure.rs:3:");
        run("pure").assert_stdout_equals("1 1 1 1\n");

        // The attribute executes again and reports the warning at its original call site.
        let pure = pure_source.replace("1 // unrelated edit", "2 // unrelated edit");
        assert_ne!(pure, pure_source);
        rfs::write("pure.rs", &pure);
        compile("pure", strategy)
            .run()
            .assert_stderr_contains("EXECUTED cached attribute")
            .assert_stderr_contains("cached attribute warning")
            .assert_stderr_contains("pure.rs:3:");
        run("pure").assert_stdout_equals("1 1 1 2\n");

        let pure = pure.replace("#[attrs::cached(1)]", "#[attrs::cached(2)]");
        rfs::write("pure.rs", &pure);
        compile("pure", strategy).run().assert_stderr_contains("EXECUTED cached attribute");
        run("pure").assert_stdout_equals("1 2 1 2\n");

        let pure = pure.replace("1 // item edit", "3 // item edit");
        rfs::write("pure.rs", &pure);
        compile("pure", strategy).run().assert_stderr_contains("EXECUTED cached attribute");
        run("pure").assert_stdout_equals("3 2 1 2\n");

        let changed_macro =
            macro_source.replace("const VERSION: u32 = 1;", "const VERSION: u32 = 2;");
        assert_ne!(changed_macro, macro_source);
        rfs::write("attrs.rs", changed_macro);
        rustc().input("attrs.rs").run();
        compile("pure", strategy).run().assert_stderr_contains("EXECUTED cached attribute");
        run("pure").assert_stdout_equals("3 2 2 2\n");
        compile("pure", strategy)
            .run()
            .assert_stderr_contains("EXECUTED cached attribute")
            .assert_stderr_contains("cached attribute warning");
        run("pure").assert_stdout_equals("3 2 2 2\n");

        // Even unchanged observations must run again: dep-info is rebuilt each compilation.
        rfs::write("observations.rs", &observation_source);
        rfs::write("tracked.txt", "1");
        for (env, expanded, expected) in
            [("1", "1", "1 1 1 false\n"), ("1", "1", "1 1 1 false\n"), ("2", "2", "2 3 2 true\n")]
        {
            if env == "2" {
                rfs::write("tracked.txt", "3");
                rfs::write(
                    "observations.rs",
                    observation_source.replace("/* first */", "/* other */"),
                );
            }
            compile("observations", strategy)
                .env("ATTR_CACHE_ENV", env)
                .env("ATTR_CACHE_EXPANDED", expanded)
                .run()
                .assert_stderr_contains("EXECUTED tracked environment")
                .assert_stderr_contains("EXECUTED tracked path")
                .assert_stderr_contains("EXECUTED source text")
                .assert_stderr_contains("EXECUTED expanded expression");
            run("observations").assert_stdout_equals(expected);
            let dep_info = rfs::read_to_string("observations.d");
            assert!(dep_info.contains(&format!("# env-dep:ATTR_CACHE_ENV={env}")), "{dep_info}");
            assert!(
                dep_info.contains(&format!("# env-dep:ATTR_CACHE_EXPANDED={expanded}")),
                "{dep_info}"
            );
            assert!(dep_info.contains("tracked.txt"), "{dep_info}");
        }

        for _ in 0..2 {
            compile("panicking", strategy)
                .run_fail()
                .assert_stderr_contains("custom attribute panicked")
                .assert_stderr_contains("attribute panic sentinel")
                .assert_stderr_not_contains("internal compiler error");
        }
    }
}
