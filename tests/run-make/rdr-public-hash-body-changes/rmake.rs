//@ ignore-cross-compile

use run_make_support::{rfs, rustc};

fn compile_and_read_public_hash(source: &str) -> String {
    rfs::write("dep.rs", source);
    rustc().input("dep.rs").arg("-Zpublic-api-hash").run();

    let listing = rustc().arg("-Zls=public_hash").input("libdep.rlib").run().stdout_utf8();

    listing
        .lines()
        .find_map(|line| line.strip_prefix("Public hash:"))
        .map(|hash| hash.trim().to_owned())
        .expect("`-Zls=public_hash` did not print a `Public hash:` line")
}

fn changed(source: &str, from: &str, to: &str) -> String {
    assert_eq!(from.len(), to.len(), "test edits must preserve all source positions");
    assert_eq!(source.matches(from).count(), 1, "edit marker must be unique");
    source.replace(from, to)
}

fn assert_hash_unchanged(baseline_source: &str, baseline_hash: &str, from: &str, to: &str) {
    let hash = compile_and_read_public_hash(&changed(baseline_source, from, to));
    assert_eq!(baseline_hash, hash, "public hash changed after replacing `{from}` with `{to}`");
}

fn assert_hash_changed(baseline_source: &str, baseline_hash: &str, from: &str, to: &str) {
    let hash = compile_and_read_public_hash(&changed(baseline_source, from, to));
    assert_ne!(
        baseline_hash, hash,
        "public hash did not change after replacing `{from}` with `{to}`",
    );
}

fn main() {
    let baseline_source = rfs::read_to_string("dep.rs");
    let baseline_hash = compile_and_read_public_hash(&baseline_source);

    // Private bodies and ordinary, non-inlinable public bodies are codegen inputs for this crate,
    // but do not affect how a downstream crate is compiled.
    assert_hash_unchanged(
        &baseline_source,
        &baseline_hash,
        "/* private */\n    1",
        "/* private */\n    2",
    );
    assert_hash_unchanged(
        &baseline_source,
        &baseline_hash,
        "pub fn ordinary() -> usize {\n    private_body_a()\n}",
        "pub fn ordinary() -> usize {\n    private_body_b()\n}",
    );

    // These bodies are encoded into metadata and can affect downstream compilation.
    assert_hash_changed(
        &baseline_source,
        &baseline_hash,
        "/* macro */\n        1",
        "/* macro */\n        2",
    );
    assert_hash_changed(&baseline_source, &baseline_hash, "/* const */ 1", "/* const */ 2");
    assert_hash_changed(
        &baseline_source,
        &baseline_hash,
        "/* inline */\n    1",
        "/* inline */\n    2",
    );
    assert_hash_changed(
        &baseline_source,
        &baseline_hash,
        "/* generic */\n    1",
        "/* generic */\n    2",
    );
}
