//@ ignore-cross-compile
// Reason: the test executes the generated binary and test harness.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use run_make_support::{rfs, run, rustc};

const FUNCTIONS: u64 = 64;
const OPERATIONS: u64 = 128;
const MULTIPLIER: u64 = 6364136223846793005;

fn source(delta: u64) -> u64 {
    let mut source = String::from(
        "#![allow(dead_code)]\n\
         static SEED: u64 = 0xa09e667f3bcc908b;\n\
         #[inline] fn helper<T: Into<u64>>(x: T) -> u64 { x.into().rotate_left(7) ^ SEED }\n",
    );
    let mut expected = 0;
    for function in 0..FUNCTIONS {
        writeln!(source, "#[inline(never)] pub fn f{function:03}(mut x: u64) -> u64 {{").unwrap();
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
) -> BTreeSet<String> {
    let mut command = rustc();
    command
        .input("fixture.rs")
        .crate_name("fixture")
        .edition("2024")
        .opt_level(optimization)
        .arg("-Zhuman-readable-cgu-names")
        .arg("-Zprint-mono-items");
    if kind == "test" {
        command.arg("--test");
    } else {
        command.crate_type(kind);
    }
    if let Some(cache) = cache {
        command.incremental(cache).arg("-Zincremental-verify-ich");
    }
    if let Some(split) = split {
        command.arg(format!("-Zohm-split-large-cgus={}", if split { "yes" } else { "no" }));
    }
    if let Some(cap) = cap {
        command.codegen_units(cap);
    }
    command
        .run()
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

fn main() {
    let expected = source(1);
    let original = compile("bin", Some("incremental"), None, "0", None);
    assert!(split(&original), "the default did not split the oversized unit: {original:?}");
    assert!(original.len() <= 256);
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
    let before = objects("incremental");

    let expected = source(7);
    let edited = compile("bin", Some("incremental"), None, "0", None);
    assert_eq!(original, edited, "a body edit changed the partition names");
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
    let after = objects("incremental");
    assert_eq!(before.keys().collect::<Vec<_>>(), after.keys().collect::<Vec<_>>());
    let reused = before.iter().filter(|(name, value)| after.get(*name) == Some(value)).count();
    assert!(reused > 0 && reused < before.len(), "expected partial native reuse: {reused}");

    let disabled = compile("bin", Some("incremental"), Some(false), "0", None);
    assert!(!split(&disabled));
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
    assert_eq!(edited, compile("bin", Some("incremental"), None, "0", None));
    run("fixture").assert_stdout_equals(format!("{expected}\n"));

    assert!(split(&compile("rlib", Some("library"), None, "0", None)));
    rfs::write("consumer.rs", "fn main() { println!(\"{}\", fixture::checksum()); }\n");
    rustc().input("consumer.rs").edition("2024").extern_("fixture", "libfixture.rlib").run();
    run("consumer").assert_stdout_equals(format!("{expected}\n"));
    assert!(split(&compile("test", Some("tests"), None, "0", None)));
    run("fixture").assert_stdout_contains("1 passed");

    assert!(!split(&compile("bin", Some("optimized"), None, "1", None)));
    assert!(!split(&compile("bin", None, None, "0", None)));
    let limited = compile("bin", Some("limited"), None, "0", Some(1));
    assert!(!split(&limited) && limited.len() == 1);
    run("fixture").assert_stdout_equals(format!("{expected}\n"));
}
