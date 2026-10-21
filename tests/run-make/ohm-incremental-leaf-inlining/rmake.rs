//@ ignore-cross-compile
// Reason: the test executes consumers of the generated library.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;

use run_make_support::regex::Regex;
use run_make_support::{rfs, run, rustc};

fn source(xor: u64) {
    let mut source = include_str!("leaf.rs").replace("value ^ 7", &format!("value ^ {xor}"));
    source.push_str("\npub fn large(mut value: u64) -> u64 {\n");
    for constant in 0..24 {
        writeln!(source, "value = (value << 1) ^ (value >> 3) ^ {constant};").unwrap();
    }
    source.push_str("value\n}\n");
    rfs::write("fixture.rs", source);
}

fn compile(
    directory: &str,
    level: &str,
    incremental: bool,
    limit: usize,
    units: usize,
    extra: &[&str],
    xor: u64,
) -> BTreeMap<String, bool> {
    let directory = Path::new(directory);
    rfs::create_dir_all(directory);
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "ll") {
            rfs::remove_file(path);
        }
    }
    let mut command = rustc();
    command
        .input("fixture.rs")
        .crate_name("leaf")
        .edition("2024")
        .opt_level(level)
        .codegen_units(units)
        .emit("llvm-ir,link")
        .out_dir(directory)
        .arg("-Clto=off")
        .arg("-Cmetadata=leaf_scope")
        .arg(format!("-Zohm-incremental-inline-threshold={limit}"))
        .args(extra);
    if incremental {
        command.incremental(directory.join("cache")).arg("-Zincremental-verify-ich");
    }
    command.run();

    let mut ir = String::new();
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "ll") {
            ir.push_str(&rfs::read_to_string(path));
        }
    }
    let function = Regex::new(r"(?ms)^define[^\n]*@(\w+_caller)\([^\n]*\).*?\{(.*?)^}").unwrap();
    let call = Regex::new(r"\b(call|invoke)\b").unwrap();
    let mut calls = BTreeMap::new();
    for captures in function.captures_iter(&ir) {
        assert!(calls.insert(captures[1].to_owned(), call.is_match(&captures[2])).is_none());
    }
    assert_eq!(calls.len(), 5, "{ir}");
    assert!(calls["never_caller"]);

    let mut consumer = format!(
        "fn main() {{ for value in [0, 1, 17, u64::MAX] {{\n\
         assert_eq!(leaf::caller::tiny_caller(value), value ^ {xor});\n\
         assert_eq!(leaf::caller::never_caller(value), value ^ 11);\n\
         assert_eq!(leaf::caller::external_caller(value), value ^ 13);\n\
         assert_eq!(leaf::caller::nonleaf_caller(value), value ^ 10);\n\
         let mut expected = value;\n",
    );
    for constant in 0..24 {
        writeln!(consumer, "expected = (expected << 1) ^ (expected >> 3) ^ {constant};").unwrap();
    }
    consumer.push_str("assert_eq!(leaf::caller::large_caller(value), expected);\n}}\n");
    rfs::write(directory.join("consumer.rs"), consumer);
    rustc()
        .input(directory.join("consumer.rs"))
        .edition("2024")
        .extern_("leaf", directory.join("libleaf.rlib"))
        .output(directory.join("consumer"))
        .run();
    run(directory.join("consumer").to_str().unwrap());
    calls
}

fn main() {
    source(7);
    for level in ["0", "1", "2", "3", "s", "z"] {
        for incremental in [false, true] {
            let directory = format!("opt{level}-incremental{incremental}");
            let off = compile(&directory, level, incremental, 0, 16, &[], 7);
            let on = compile(&directory, level, incremental, 8, 16, &[], 7);
            if level == "1" && incremental {
                assert!(off["tiny_caller"]);
                assert!(!on["tiny_caller"]);
                for name in ["never_caller", "external_caller", "nonleaf_caller", "large_caller"] {
                    assert!(on[name], "{name}");
                }
            } else {
                assert_eq!(off, on);
            }
        }
    }
    let single_off = compile("single", "1", true, 0, 1, &[], 7);
    assert_eq!(single_off, compile("single", "1", true, 8, 1, &[], 7));
    for limit in [1, 2, 8] {
        let calls = compile("threshold", "1", true, limit, 16, &[], 7);
        assert_eq!(calls["tiny_caller"], limit == 1);
    }
    for limit in ["never", "1", "2"] {
        let option = format!("-Zcross-crate-inline-threshold={limit}");
        let calls = compile("global-limit", "1", true, 8, 16, &[&option], 7);
        assert_eq!(calls["tiny_caller"], limit != "2");
    }
    for limit in [0, 8, 0] {
        let calls = compile("reuse", "1", true, limit, 16, &[], 7);
        assert_eq!(calls["tiny_caller"], limit == 0);
    }
    for xor in [7, 9, 7] {
        source(xor);
        let calls = compile("reuse", "1", true, 8, 16, &[], xor);
        assert!(!calls["tiny_caller"]);
    }
}
