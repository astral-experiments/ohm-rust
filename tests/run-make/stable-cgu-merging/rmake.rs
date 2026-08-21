//@ ignore-cross-compile
// Reason: the compiled executable is run before and after an incremental edit.

use std::collections::{BTreeMap, BTreeSet};

use run_make_support::{rfs, run, rustc};

const MODULES: [&str; 9] =
    ["changed", "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel"];

fn compile(incremental: Option<&str>, stable_merging: bool) -> BTreeMap<String, String> {
    let mut command = rustc();
    command
        .input("main.rs")
        .codegen_units(4)
        .arg("-Zhuman-readable-cgu-names")
        .arg("-Zprint-mono-items");

    if let Some(incremental) = incremental {
        command.incremental(incremental);
    }
    if stable_merging {
        command.arg("-Zohm-stable-cgu-merging");
    }

    let output = command.run().stdout_utf8();
    MODULES
        .into_iter()
        .map(|module| {
            let codegen_unit = output
                .lines()
                .find(|line| line.contains(&format!("{module}::value")))
                .and_then(|line| line.split_once("@@"))
                .and_then(|(_, units)| units.split_whitespace().next())
                .unwrap_or_else(|| panic!("missing `{module}` codegen unit:\n{output}"));
            (module.to_owned(), codegen_unit.to_owned())
        })
        .collect()
}

fn main() {
    // The source has more original module CGUs than its explicitly requested limit.
    let ordinary = compile(Some("ordinary"), false);
    assert!(ordinary.values().collect::<BTreeSet<_>>().len() <= 4);
    run("main").assert_stdout_contains("45");

    let original = compile(Some("stable"), true);
    let distinct_units: BTreeSet<_> = original.values().collect();
    assert!(distinct_units.len() > 1 && distinct_units.len() <= 4, "{original:?}");
    run("main").assert_stdout_contains("45");

    // Grow one function and add a call to an existing root. Its body must be recompiled, but
    // independent modules must retain both their bucket membership and work-product identity.
    let source = rfs::read_to_string("main.rs");
    let edited = source.replace("number // EDIT", "number + super::alpha::value(0) // EDIT");
    assert_ne!(source, edited);
    rfs::write("main.rs", edited);

    let ordinary_rebuilt = compile(Some("ordinary"), false);
    assert!(ordinary_rebuilt.values().collect::<BTreeSet<_>>().len() <= 4);
    run("main").assert_stdout_contains("46");

    let rebuilt = compile(Some("stable"), true);
    assert_eq!(original, rebuilt, "an unrelated CGU moved after changing one function");
    run("main").assert_stdout_contains("46");

    // Explicit counts remain limits, and enabling the tracked flag does not break ordinary
    // non-incremental code generation.
    let nonincremental = compile(None, true);
    assert!(nonincremental.values().collect::<BTreeSet<_>>().len() <= 4);
    run("main").assert_stdout_contains("46");

    // A single explicitly requested codegen unit must remain valid with stable merging.
    rustc()
        .input("main.rs")
        .incremental("single-unit")
        .codegen_units(1)
        .arg("-Zohm-caller-local-cgus")
        .arg("-Zohm-stable-cgu-merging")
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich")
        .run();
    run("main").assert_stdout_contains("46");
}
