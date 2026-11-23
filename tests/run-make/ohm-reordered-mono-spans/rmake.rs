//@ ignore-cross-compile

use run_make_support::rustc;

fn compile(revision: &str, cache: &str) -> String {
    let mut command = rustc();
    command
        .input("main.rs")
        .incremental(cache)
        .cfg(revision)
        .arg("-Zohm-incremental-mono-partitions")
        .arg("-Zpublic-api-hash=no")
        .arg("-Zquery-dep-graph")
        .arg("-Zincremental-verify-ich");
    if revision == "fail" { command.run_fail().stderr_utf8() } else { command.run().stderr_utf8() }
}

fn main() {
    compile("first", "warm");
    // The caller changes order and spans. Its partition is still green.
    compile("second", "warm");
    // Changing only a generic callee forces collection to read the saved
    // caller edges. The error must name the current instantiation site.
    let warm = compile("fail", "warm");
    let clean = compile("fail", "clean");
    assert!(clean.contains("The diagnostic must point here."), "{clean}");
    assert_eq!(warm, clean);
}
