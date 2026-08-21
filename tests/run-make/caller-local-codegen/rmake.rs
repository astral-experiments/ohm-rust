//@ ignore-cross-compile
// Reason: the compiled executable is run before and after an incremental edit.

use run_make_support::{rfs, run, rustc};

fn compile(incremental: Option<&str>, caller_local: bool) -> String {
    let mut command = rustc();
    command
        .input("main.rs")
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

    command.run().stdout_utf8()
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

    let ordinary = compile(Some("ordinary"), false);
    assert_placement(&ordinary, "upstream::apply::<", "upstream.", ".volatile[");
    assert_placement(&ordinary, "upstream::invoke::<", "upstream.", ".volatile[");
    run("main").assert_stdout_contains("dropped:false:true:7:11");

    let caller_local = compile(Some("caller-local"), true);
    for function in ["upstream::apply::<", "upstream::invoke::<"] {
        assert_placement(&caller_local, function, "main.", "-first.volatile[");
        assert_placement(&caller_local, function, "main.", "-second.volatile[");
    }
    assert_placement(&caller_local, "upstream::identity::<u64>", "upstream.", ".volatile[");
    assert_fallback_placement(&caller_local, &["<upstream::Wrapper<", ">::drop"]);
    assert_fallback_placement(&caller_local, &["drop_glue::<upstream::Wrapper<"]);
    run("main").assert_stdout_contains("dropped:false:true:7:11");

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
    assert_placement(&rebuilt, "upstream::apply::<", "main.", "-first.volatile[");
    run("main").assert_stdout_contains("dropped:true:true:7:11");
}
