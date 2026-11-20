//@ ignore-cross-compile
// Reason: the compiled executable is run before and after an incremental edit.

use run_make_support::{Rustc, rfs, run, rustc};

fn compiler(incremental: Option<&str>, caller_local: bool) -> Rustc {
    let mut command = rustc();
    command
        .edition("2021")
        .codegen_units(32)
        .arg("-Zhuman-readable-cgu-names")
        .arg("-Zprint-mono-items")
        .arg("-Zshare-generics=yes")
        .arg("-Zpublic-api-hash")
        .extern_("upstream", "libupstream.rlib");

    if let Some(incremental) = incremental {
        command.incremental(incremental);
    }
    if caller_local {
        command.arg("-Zohm-caller-local-cgus");
    }

    command
}

fn compile(incremental: Option<&str>, caller_local: bool) -> String {
    compiler(incremental, caller_local).input("main.rs").run().stdout_utf8()
}

fn assert_placement(output: &str, function: &str, codegen_unit_owner: &str, module: &str) {
    assert!(
        output.lines().any(|line| {
            line.contains(function)
                && line
                    .split_once("@@")
                    .is_some_and(|(_, units)| units.trim_start().starts_with(codegen_unit_owner))
                && line.contains(module)
        }),
        "did not find `{function}` in `{codegen_unit_owner}...{module}`:\n{output}",
    );
}

fn assert_fallback_placement(output: &str, fragments: &[&str]) {
    assert!(
        output.lines().any(|line| {
            fragments.iter().all(|fragment| line.contains(fragment))
                && line.split_once("@@").is_some_and(|(_, units)| units.contains("fallback.cgu["))
        }),
        "did not find `{fragments:?}` in the fallback codegen unit:\n{output}",
    );
}

fn main() {
    rustc().input("upstream.rs").arg("-Zshare-generics=yes").arg("-Zpublic-api-hash").run();

    let coroutine_glue = "drop_glue::<{async fn body of tasks::pending()}>";

    let ordinary = compile(Some("ordinary"), false);
    assert_placement(&ordinary, "upstream::apply::<", "upstream.", ".volatile[");
    assert_placement(&ordinary, "upstream::invoke::<", "upstream.", ".volatile[");
    assert_placement(&ordinary, "upstream::identity::<u64>", "upstream.", ".volatile[");
    assert_fallback_placement(&ordinary, &[coroutine_glue]);
    assert_fallback_placement(&ordinary, &["<upstream::Wrapper<", ">::drop"]);
    assert_fallback_placement(&ordinary, &["drop_glue::<upstream::Wrapper<"]);
    run("main").assert_stdout_contains("dropped:false:true:7:11");

    let caller_local = compile(Some("caller-local"), true);
    assert_placement(&caller_local, "::evaluate_first", "main.", "-first[");
    assert_placement(&caller_local, coroutine_glue, "main.", "-tasks[");
    for function in ["upstream::apply::<", "upstream::invoke::<"] {
        assert_placement(&caller_local, function, "main.", "-first[");
        assert_placement(&caller_local, function, "main.", "-second[");
    }
    assert_placement(&caller_local, "upstream::identity::<u64>", "upstream.", ".volatile[");
    for function in ["as std::ops::Drop>::drop", "drop_glue::<upstream::Wrapper<"] {
        assert_placement(&caller_local, function, "main.", "-tasks[");
    }
    run("main").assert_stdout_contains("dropped:false:true:7:11");

    // In an rlib the same specialization still follows its sole caller.
    rfs::write("library.rs", "include!(\"main.rs\"); pub fn entry() { main() }");
    let library =
        compiler(Some("library"), true).input("library.rs").crate_type("rlib").run().stdout_utf8();
    assert_placement(&library, "upstream::identity::<u64>", "library.", "-second[");

    // The flag is inert without incremental compilation.
    let nonincremental = compile(None, true);
    assert!(
        !nonincremental.lines().any(|line| {
            line.contains("upstream::apply::<") && line.contains("-first.volatile[")
        }),
        "caller-local partitioning unexpectedly affected a nonincremental build",
    );

    let original = rfs::read_to_string("main.rs");
    let changed = original.replace("number >= 3", "number <  3");
    assert_ne!(original, changed);
    rfs::write("main.rs", changed);

    let rebuilt = compile(Some("caller-local"), true);
    assert_placement(&rebuilt, "upstream::apply::<", "main.", "-first[");
    run("main").assert_stdout_contains("dropped:true:true:7:11");

    // Add a second user in another module, then remove it in the same cache.
    let shared = original.replace("number >= 3", "upstream::identity(number as u64) == 2");
    rfs::write("main.rs", shared);
    let rebuilt = compile(Some("caller-local"), true);
    assert_placement(&rebuilt, "upstream::identity::<u64>", "upstream.", ".volatile[");
    run("main").assert_stdout_contains("dropped:true:true:7:11");

    rfs::write("main.rs", original);
    let rebuilt = compile(Some("caller-local"), true);
    assert_placement(&rebuilt, "upstream::identity::<u64>", "upstream.", ".volatile[");
    run("main").assert_stdout_contains("dropped:false:true:7:11");
}
