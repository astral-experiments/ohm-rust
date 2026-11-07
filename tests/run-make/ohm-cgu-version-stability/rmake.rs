//@ ignore-cross-compile
// Reason: the generated executable is run under both compiler identities.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use run_make_support::{rfs, run, rustc};

fn source(modules: usize, functions: usize, operations: usize) -> Vec<String> {
    let mut source = String::new();
    let mut roots = Vec::new();
    for module in 0..modules {
        writeln!(source, "mod module_{module} {{").unwrap();
        for function in 0..functions {
            writeln!(source, "#[inline(never)] pub fn f{function:03}(mut x: u64) -> u64 {{")
                .unwrap();
            for operation in 0..operations {
                writeln!(
                    source,
                    "x = x.wrapping_mul(6364136223846793005).wrapping_add({});",
                    operation + function + module + 1,
                )
                .unwrap();
            }
            source.push_str("x\n}\n");
            roots.push(format!("module_{module}::f{function:03}"));
        }
        source.push_str("}\n");
    }
    source.push_str("fn main() { let mut result = 0u64;\n");
    for root in &roots {
        writeln!(source, "result ^= {root}(std::hint::black_box(7));").unwrap();
    }
    source.push_str("println!(\"{result}\"); }\n");
    rfs::write("fixture.rs", source);
    roots
}

fn compile(
    case: &str,
    version: &str,
    roots: &[String],
    units: usize,
    split: bool,
) -> BTreeMap<String, BTreeSet<String>> {
    let output = rustc()
        .input("fixture.rs")
        .crate_name("fixture")
        .opt_level("1")
        .codegen_units(units)
        .incremental(format!("{case}-{version}"))
        .env("RUSTC_FORCE_RUSTC_VERSION", version)
        .arg("-Zohm-stable-cgu-merging")
        .arg(format!("-Zohm-split-large-cgus={}", if split { "yes" } else { "no" }))
        .arg("-Zprint-mono-items")
        .run()
        .stdout_utf8();
    let mut groups = BTreeMap::<_, BTreeSet<_>>::new();
    for root in roots {
        let line = output
            .lines()
            .find(|line| line.contains(root))
            .unwrap_or_else(|| panic!("missing {root}:\n{output}"));
        let (_, unit) = line.split_once("@@").unwrap();
        let unit = unit.split_whitespace().next().unwrap().split('[').next().unwrap();
        groups.entry(unit.to_owned()).or_default().insert(root.clone());
    }
    assert!(groups.len() > 1 && groups.len() <= units, "{groups:?}");
    groups
}

fn main() {
    let mut moved = Vec::new();
    for (case, modules, functions, operations, units, split) in
        [("merge", 9, 2, 8, 4, false), ("split", 2, 64, 256, 16, true)]
    {
        let roots = source(modules, functions, operations);
        let before = compile(case, "version-one", &roots, units, split);
        let expected = run("fixture").stdout_utf8();
        let after = compile(case, "version-two", &roots, units, split);
        run("fixture").assert_stdout_equals(expected);
        assert_ne!(before.keys().collect::<Vec<_>>(), after.keys().collect::<Vec<_>>());
        // Artifact names retain their version identity; compare the actual groups of roots.
        if before.into_values().collect::<BTreeSet<_>>()
            != after.into_values().collect::<BTreeSet<_>>()
        {
            moved.push(case);
        }
    }
    assert!(moved.is_empty(), "compiler version changed these partitions: {moved:?}");
}
