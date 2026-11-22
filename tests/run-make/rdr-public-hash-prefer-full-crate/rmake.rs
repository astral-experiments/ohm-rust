//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn hashes(rlib: &str) -> (String, String) {
    let info = rustc().input(rlib).arg("-Zls=root").run().stdout_utf8();
    let field = |key| {
        info.lines()
            .find_map(|line| line.strip_prefix(key))
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned()
    };
    (field("public_hash "), field("hash "))
}

fn leaf(hidden: &str) -> (String, String) {
    rfs::write("hidden.rs", hidden);
    rustc().input("leaf.rs").crate_type("rlib").arg("-Zpublic-api-hash").run();
    hashes("libleaf.rlib")
}

fn bridge(marker: &str, flag: bool, reused: bool) -> (String, String) {
    rfs::write(
        "bridge.rs",
        format!(
            "pub struct {marker};\npub use leaf::Token;\n\
             #[inline] pub fn answer(t: Token) -> u8 {{ leaf::value(t) }}"
        ),
    );
    let mut compiler = rustc();
    compiler
        .input("bridge.rs")
        .edition("2024")
        .crate_type("rlib")
        .extern_("leaf", "libleaf.rlib")
        .incremental("cache")
        .arg("-Zpublic-api-hash");
    if flag {
        compiler.arg("-Zohm-prefer-full-crate-hash");
    }
    if reused {
        compiler.arg("-Zassert-incr-state=loaded");
    }
    compiler.run();
    hashes("libbridge.rlib")
}

fn consume(expected: &str) {
    rustc()
        .input("main.rs")
        .edition("2024")
        .extern_("bridge", "libbridge.rlib")
        .arg("-Zpublic-api-hash")
        .run();
    run("main").assert_stdout_contains(expected);
}

fn main() {
    rfs::write(
        "leaf.rs",
        "mod hidden;\npub struct Token;\n\
         #[inline(never)] pub fn value(_: Token) -> u8 { hidden::value() }",
    );
    rfs::write("main.rs", r#"fn main() { println!("{}", bridge::answer(bridge::Token)); }"#);
    let before = leaf("pub(super) fn value() -> u8 { 17 }");
    let (public, private) = bridge("First", false, false);
    assert_ne!(public, private);
    let (public, private) = bridge("Second", true, true);
    assert_eq!(public, private);
    consume("17");

    // Adding a private definition can move leaf::Token's DefIndex. The already
    // built bridge must keep resolving leaf by public SVH and Token by DefPathHash.
    let after = leaf("fn extra() {} pub(super) fn value() -> u8 { 29 }");
    assert_eq!(before.0, after.0);
    assert_ne!(before.1, after.1);
    consume("29");

    let (public, private) = bridge("Third", false, true);
    assert_ne!(public, private);
    consume("29");
}
