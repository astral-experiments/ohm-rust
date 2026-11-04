//@ ignore-cross-compile
// Reason: the test executes the generated binary and test harness.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use run_make_support::{dynamic_lib_name, rfs, run, run_in_tmpdir, rustc};

const FUNCTIONS: u64 = 64;
const OPERATIONS: u64 = 256;
const MULTIPLIER: u64 = 6364136223846793005;

fn source(delta: u64, available_externally: bool) -> u64 {
    let mut source = String::from(
        "#![feature(linkage)]\n\
         #![allow(dead_code, unused_attributes)]\n\
         static SEED: u64 = 0xa09e667f3bcc908b;\n\
         #[inline(always)] fn leaf(x: u64) -> u64 { x.rotate_left(7) ^ SEED }\n\
         #[inline(always)] fn helper<T: Into<u64>>(x: T) -> u64 { leaf(x.into()) }\n",
    );
    let mut expected = 0;
    for function in 0..FUNCTIONS {
        if available_externally && function < 16 {
            source.push_str("#[linkage = \"available_externally\"]\n#[inline(always)]\n");
        } else {
            source.push_str("#[inline(never)]\n");
        }
        writeln!(source, "pub fn f{function:03}(mut x: u64) -> u64 {{").unwrap();
        let mut value = function + 11;
        for operation in 0..OPERATIONS {
            let add = if function == 0 && operation == 0 {
                delta
            } else {
                function * OPERATIONS + operation + 1
            };
            writeln!(source, "x = x.wrapping_mul({MULTIPLIER}).wrapping_add({add});").unwrap();
            value = value.wrapping_mul(MULTIPLIER).wrapping_add(add);
        }
        source.push_str("helper(x)\n}\n");
        expected ^= value.rotate_left(7) ^ 0xa09e667f3bcc908b;
    }
    source.push_str("pub fn checksum() -> u64 {\nlet mut value = 0;\n");
    for function in 0..FUNCTIONS {
        writeln!(source, "value ^= f{function:03}({});", function + 11).unwrap();
    }
    source.push_str("value\n}\nfn main() { println!(\"{}\", checksum()); }\n");
    writeln!(source, "#[test] fn checksum_test() {{ assert_eq!(checksum(), {expected}); }}")
        .unwrap();
    rfs::write("fixture.rs", source);
    expected
}

fn compile(
    kind: &str,
    cache: Option<&str>,
    split: Option<bool>,
    optimization: &str,
    cap: Option<usize>,
    split_limit: Option<usize>,
) -> BTreeSet<String> {
    let mut command = rustc();
    command
        .input("fixture.rs")
        .crate_name("fixture")
        .edition("2024")
        .opt_level(optimization)
        .arg("-Zhuman-readable-cgu-names");
    if kind == "test" {
        command.arg("--test");
    } else {
        command.crate_type(kind);
    }
    if let Some(cache) = cache {
        command
            .incremental(cache)
            .arg("-Zohm-incremental-mono-partitions")
            .arg("-Zincremental-verify-ich");
    } else {
        command.arg("-Zprint-mono-items");
    }
    if let Some(split) = split {
        command.arg(format!("-Zohm-split-large-cgus={}", if split { "yes" } else { "no" }));
    }
    if let Some(limit) = split_limit {
        command.arg(format!("-Zohm-cgu-split-limit={limit}"));
    }
    if let Some(cap) = cap {
        command.codegen_units(cap);
    }
    let output = command.run();
    if let Some(cache) = cache {
        return objects(cache).into_keys().map(|name| name.to_str().unwrap().to_owned()).collect();
    }
    output
        .stdout_utf8()
        .lines()
        .filter_map(|line| line.split_once("@@"))
        .flat_map(|(_, units)| units.split_whitespace())
        .map(|unit| unit.split('[').next().unwrap().to_owned())
        .collect()
}

fn split(units: &BTreeSet<String>) -> bool {
    units.iter().any(|unit| unit.contains("-ohm-split-"))
}

fn objects(cache: &str) -> BTreeMap<PathBuf, (SystemTime, Vec<u8>)> {
    let crate_dir = fs::read_dir(cache)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.is_dir())
        .unwrap();
    let session = fs::read_dir(crate_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir() && !path.to_string_lossy().ends_with("-working"))
        .max()
        .unwrap();
    fs::read_dir(session)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "o"))
        .map(|path| {
            let value =
                (fs::metadata(&path).unwrap().modified().unwrap(), fs::read(&path).unwrap());
            (Path::new(path.file_name().unwrap()).to_path_buf(), value)
        })
        .collect()
}

fn check(optimization: &str) {
    let expected = source(1, false);
    let original = compile("bin", Some("incremental"), None, optimization, None, None);
    assert!(split(&original), "the default did not split the oversized unit: {original:?}");
    assert!(original.len() <= 256);
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
    let before = objects("incremental");
    assert_eq!(original, compile("bin", Some("incremental"), None, optimization, None, None));
    assert_eq!(before, objects("incremental"), "an unchanged build regenerated native objects");

    let expected = source(7, false);
    let edited = compile("bin", Some("incremental"), None, optimization, None, None);
    assert_eq!(original, edited, "a body edit changed the partition names");
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
    let after = objects("incremental");
    assert_eq!(before.keys().collect::<Vec<_>>(), after.keys().collect::<Vec<_>>());
    let reused = before.iter().filter(|(name, value)| after.get(*name) == Some(value)).count();
    assert!(reused > 0 && reused < before.len(), "expected partial native reuse: {reused}");

    let disabled = compile("bin", Some("incremental"), Some(false), optimization, None, None);
    assert!(!split(&disabled));
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
    assert_eq!(edited, compile("bin", Some("incremental"), None, optimization, None, None));
    run("fixture").assert_stdout_equals(format!("{expected}\n"));

    let limited = compile("bin", Some("incremental"), None, optimization, Some(1), None);
    assert!(!split(&limited) && limited.len() == 1);
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
    assert_eq!(edited, compile("bin", Some("incremental"), None, optimization, None, None));
    run("fixture").assert_stdout_equals(format!("{expected}\n"));

    for limit in [8, 4, 2, 1] {
        let units = compile("bin", Some("incremental"), None, optimization, None, Some(limit));
        assert_eq!(split(&units), limit > 1);
        let mut shards = BTreeMap::new();
        for unit in &units {
            if let Some((source, _)) = unit.split_once("-ohm-split-") {
                *shards.entry(source).or_insert(0) += 1;
            }
        }
        assert!(shards.values().all(|&count| count <= limit), "{units:?}");
        assert!(units.len() <= 256);
        run("fixture").assert_stdout_equals(format!("{expected}\n"));
    }
    assert!(!split(&compile("bin", Some("incremental"), Some(false), optimization, None, Some(8))));
    assert_eq!(edited, compile("bin", Some("incremental"), None, optimization, None, None));

    assert!(split(&compile("rlib", Some("library"), None, optimization, None, None)));
    rfs::write("consumer.rs", "fn main() { println!(\"{}\", fixture::checksum()); }\n");
    rustc().input("consumer.rs").edition("2024").extern_("fixture", "libfixture.rlib").run();
    run("consumer").assert_stdout_equals(format!("{expected}\n"));
    assert!(split(&compile("test", Some("tests"), None, optimization, None, None)));
    run("fixture").assert_stdout_contains("1 passed");

    assert!(!split(&compile("bin", None, None, optimization, None, None)));
    let expected = source(9, true);
    assert!(!split(&compile("bin", Some("available"), Some(false), optimization, None, None)));
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
    assert!(!split(&compile("bin", Some("available"), None, optimization, None, None)));
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
}

/// Check that one real derive can move each receiver's method independently.
fn derived_receivers() {
    rustc().input("methods.rs").edition("2024").crate_type("proc-macro").run();
    let build = |cache: &str, limit: Option<usize>| {
        let mut compiler = rustc();
        compiler
            .input("derived.rs")
            .edition("2024")
            .extern_("methods", dynamic_lib_name("methods"))
            .opt_level("1")
            .codegen_units(64)
            .incremental(cache)
            .arg("-Zhuman-readable-cgu-names");
        if let Some(limit) = limit {
            compiler.arg(format!("-Zohm-cgu-split-limit={limit}"));
        }
        compiler.run();
        objects(cache)
            .into_keys()
            .filter(|name| name.to_str().unwrap().contains("-ohm-derived-"))
            .collect::<BTreeSet<_>>()
    };

    let original = build("derive", None);
    assert_eq!(original.len(), 3);
    run("derived").assert_stdout_equals("57\n102\n");
    assert!(build("no-derive-shards", Some(1)).is_empty());
    assert_eq!(build("one-extra-shard", Some(2)).len(), 1);

    let source = rfs::read_to_string("derived.rs");
    rfs::write("derived.rs", source.replace("i64 = 11;", "i64 = 21;"));
    assert_eq!(build("derive", None), original);
    run("derived").assert_stdout_equals("67\n112\n");
    assert_eq!(build("fresh-derive", None), original);
    run("derived").assert_stdout_equals("67\n112\n");
}

fn main() {
    derived_receivers();
    rfs::write("fixture.rs", "fn main() {}\n");
    rustc()
        .input("fixture.rs")
        .arg("-Zohm-cgu-split-limit=0")
        .run_fail()
        .assert_stderr_contains("incorrect value");
    for optimization in ["0", "1"] {
        run_in_tmpdir(|| check(optimization));
    }
    let expected = source(1, false);
    for optimization in ["2", "3", "s", "z"] {
        assert!(!split(&compile("bin", Some("optimized"), None, optimization, None, None)));
        run("fixture").assert_stdout_equals(format!("{expected}\n"));
    }
}
