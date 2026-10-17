//@ ignore-cross-compile
// Reason: the test executes the generated binary and test harness.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use run_make_support::{rfs, run, run_in_tmpdir, rustc};

const FUNCTIONS: u64 = 64;
const OPERATIONS: u64 = 256;
const MULTIPLIER: u64 = 6364136223846793005;

fn source(delta: u64, available_externally: bool) -> u64 {
    source_with_helper(delta, available_externally, 0)
}

fn source_with_helper(delta: u64, available_externally: bool, helper_operations: u64) -> u64 {
    let mut source = String::from(
        "#![feature(linkage)]\n\
         #![allow(dead_code, unused_attributes)]\n\
         static SEED: u64 = 0xa09e667f3bcc908b;\n",
    );
    if helper_operations == 0 {
        source.push_str("#[inline(always)] fn leaf(x: u64) -> u64 { x.rotate_left(7) ^ SEED }\n");
    } else {
        source.push_str("#[inline] fn leaf(mut x: u64) -> u64 {\n");
        for operation in 0..helper_operations {
            writeln!(source, "x = x.wrapping_mul({MULTIPLIER}).wrapping_add({});", operation + 1)
                .unwrap();
        }
        source.push_str("x.rotate_left(7) ^ SEED\n}\n");
    }
    source.push_str("#[inline(always)] fn helper<T: Into<u64>>(x: T) -> u64 { leaf(x.into()) }\n");
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
        for operation in 0..helper_operations {
            value = value.wrapping_mul(MULTIPLIER).wrapping_add(operation + 1);
        }
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
    compile_with_options(kind, cache, split, optimization, cap, split_limit, &[])
}

fn compile_with_options(
    kind: &str,
    cache: Option<&str>,
    split: Option<bool>,
    optimization: &str,
    cap: Option<usize>,
    split_limit: Option<usize>,
    options: &[&str],
) -> BTreeSet<String> {
    let mut command = rustc();
    command
        .input("fixture.rs")
        .crate_name("fixture")
        .edition("2024")
        .opt_level(optimization)
        .arg("-Zhuman-readable-cgu-names")
        .args(options);
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

fn check_growth(optimization: &str) {
    let options = ["-Zohm-cgu-split-max-growth=10", "-Zohm-incremental-inline-threshold=8"];
    let build = |kind, cache| {
        compile_with_options(kind, Some(cache), None, optimization, None, None, &options)
    };
    let expected = source(1, false);
    let original = build("bin", "growth");
    assert!(split(&original));
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
    let before = objects("growth");
    assert_eq!(original, build("bin", "growth"));
    assert_eq!(before, objects("growth"));

    let expected = source(7, false);
    assert_eq!(original, build("bin", "growth"));
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
    let after = objects("growth");
    let reused = before.iter().filter(|(name, value)| after.get(*name) == Some(value)).count();
    assert!(reused > 0 && reused < before.len(), "expected partial native reuse: {reused}");

    assert!(split(&build("rlib", "library-growth")));
    rfs::write("consumer.rs", "fn main() { println!(\"{}\", fixture::checksum()); }\n");
    rustc().input("consumer.rs").edition("2024").extern_("fixture", "libfixture.rlib").run();
    run("consumer").assert_stdout_equals(format!("{expected}\n"));
    assert!(split(&build("test", "test-growth")));
    run("fixture").assert_stdout_contains("1 passed");
}

fn check_expensive_copies() {
    let expected = source_with_helper(1, false, 4096);
    for (limit, expected_split) in [
        (None, true),
        (Some(10), false),
        (Some(0), false),
        (Some(usize::MAX), true),
        (Some(10), false),
    ] {
        let mut options = vec!["-Zohm-incremental-inline-threshold=8".to_owned()];
        if let Some(limit) = limit {
            options.push(format!("-Zohm-cgu-split-max-growth={limit}"));
        }
        let options: Vec<_> = options.iter().map(String::as_str).collect();
        let units = compile_with_options("bin", Some("expensive"), None, "1", None, None, &options);
        assert_eq!(split(&units), expected_split, "growth limit: {limit:?}");
        run("fixture").assert_stdout_equals(format!("{expected}\n"));
    }
}

fn main() {
    rfs::write("fixture.rs", "fn main() {}\n");
    rustc()
        .input("fixture.rs")
        .arg("-Zohm-cgu-split-limit=0")
        .run_fail()
        .assert_stderr_contains("incorrect value");
    rustc()
        .input("fixture.rs")
        .arg("-Zohm-cgu-split-max-growth=-1")
        .run_fail()
        .assert_stderr_contains("incorrect value");
    for optimization in ["0", "1"] {
        run_in_tmpdir(|| check(optimization));
        run_in_tmpdir(|| check_growth(optimization));
    }
    run_in_tmpdir(check_expensive_copies);
    let expected = source(1, false);
    for optimization in ["2", "3", "s", "z"] {
        assert!(!split(&compile("bin", Some("optimized"), None, optimization, None, None)));
        run("fixture").assert_stdout_equals(format!("{expected}\n"));
    }
}
