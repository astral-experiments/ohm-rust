//@ needs-target-std

use run_make_support::{rfs, rustc, serde_json};
use serde_json::Value;

fn diagnostics(cache: &str, threads: usize) -> Vec<Value> {
    let output = rustc()
        .input("lib.rs")
        .crate_type("rlib")
        .edition("2024")
        .emit("metadata")
        .incremental(cache)
        .error_format("json")
        .arg("-Wmissing_docs")
        .arg("-Wasync_fn_in_trait")
        .arg("-Aunused")
        .arg("-Zincremental-verify-ich")
        .arg(format!("-Zthreads={threads}"))
        .run();
    let mut diagnostics: Vec<Value> = output
        .stderr_utf8()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|value: &Value| value["$message_type"] == "diagnostic")
        .map(|mut value| {
            value.as_object_mut().unwrap().remove("rendered");
            value
        })
        .collect();
    diagnostics.sort_by_key(Value::to_string);
    diagnostics
}

fn main() {
    rfs::write(
        "hidden.rs",
        "pub struct S; impl S { pub fn f(&self) {} }\npub trait T { async fn f(); }\n",
    );
    rfs::write("stable.rs", "/// A documented, unchanged item.\npub struct Stable;\n");
    let root = "//! A crate used to check visibility-dependent diagnostics.\nmod hidden;\n/// An unchanged module.\npub mod stable;\n";
    for threads in [1, 4] {
        let cache = format!("incremental-{threads}");
        for (index, (source, expected_docs, expected_async)) in [
            (root.to_owned(), 0, 0),
            (root.to_owned(), 0, 0),
            (format!("{root}\n/// An unrelated exported item.\npub fn unrelated() {{}}\n"), 0, 0),
            (format!("{root}\npub use hidden::{{S, T}};\n"), 4, 1),
            (format!("{root}\npub use hidden::{{S, T}};\n"), 4, 1),
            (root.to_owned(), 0, 0),
            (root.replace("mod hidden;", "/// A newly public module.\npub mod hidden;"), 4, 1),
        ]
        .into_iter()
        .enumerate()
        {
            rfs::write("lib.rs", source);
            let cached = diagnostics(&cache, threads);
            let fresh = diagnostics(&format!("fresh-{threads}-{index}"), threads);
            assert_eq!(cached, fresh);
            for (lint, expected) in
                [("missing_docs", expected_docs), ("async_fn_in_trait", expected_async)]
            {
                assert_eq!(
                    cached.iter().filter(|value| value["code"]["code"] == lint).count(),
                    expected
                );
            }
        }
    }
}
