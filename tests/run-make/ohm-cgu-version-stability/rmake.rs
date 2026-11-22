//@ ignore-cross-compile
// Reason: the generated executable is run under both compiler identities.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use run_make_support::{rfs, run, rustc};

fn source(modules: usize, functions: usize, operations: usize, tagged: bool) -> Vec<String> {
    let mut source = String::new();
    let mut roots = Vec::new();
    for module in 0..modules {
        writeln!(source, "mod module_{module} {{").unwrap();
        if tagged {
            source.push_str("#[inline(never)] fn tag<const TAG: u64>(x: u64) -> u64 { x ^ TAG }\n");
        }
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
            if tagged {
                writeln!(source, "tag::<{function}>(x)\n}}").unwrap();
            } else {
                source.push_str("x\n}\n");
            }
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
    units: Option<usize>,
    split: bool,
    opt_level: &str,
    caller_local: bool,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut command = rustc();
    if let Some(limit) = units {
        command.codegen_units(limit);
    }
    let output = command
        .input("fixture.rs")
        .crate_name("fixture")
        .opt_level(opt_level)
        .incremental(format!("{case}-{version}"))
        .env("RUSTC_FORCE_RUSTC_VERSION", version)
        .arg(format!("-Zohm-caller-local-cgus={}", if caller_local { "yes" } else { "no" }))
        .arg("-Zohm-stable-cgu-merging")
        .arg(format!("-Zohm-split-large-cgus={}", if split { "yes" } else { "no" }))
        .arg("-Zprint-mono-items")
        .run()
        .stdout_utf8();
    fn unit(line: &str) -> &str {
        let (_, unit) = line.split_once("@@").unwrap();
        unit.split_whitespace().next().unwrap().split('[').next().unwrap()
    }
    if split && opt_level == "0" {
        let tags: BTreeSet<_> =
            output.lines().filter(|line| line.contains("module_0::tag::<")).map(unit).collect();
        assert_eq!(tags.len(), 1, "instances of tag should share a shard: {tags:?}");
    }
    let mut groups = BTreeMap::<_, BTreeSet<_>>::new();
    for root in roots {
        let line = output
            .lines()
            .find(|line| line.contains(root))
            .unwrap_or_else(|| panic!("missing {root}:\n{output}"));
        groups.entry(unit(line).to_owned()).or_default().insert(root.clone());
    }
    if let Some(limit) = units {
        assert!(groups.len() > 1 && groups.len() <= limit, "{groups:?}");
    } else {
        assert_eq!(groups.len(), roots.len());
    }
    groups
}

fn main() {
    let mut moved = Vec::new();
    for (case, modules, functions, operations, units, split, opt_level) in [
        ("merge", 9, 2, 8, Some(4), false, "1"),
        ("split", 2, 64, 256, Some(16), true, "1"),
        ("caller-local", 2, 64, 256, Some(16), true, "0"),
        ("default", 257, 1, 1, None, false, "1"),
    ] {
        let roots = source(modules, functions, operations, split);
        // Optimized splits must keep the same groups with caller-local placement enabled.
        let before =
            compile(case, "version-one", &roots, units, split, opt_level, opt_level == "0");
        let expected = run("fixture").stdout_utf8();
        let after = compile(case, "version-two", &roots, units, split, opt_level, split);
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
