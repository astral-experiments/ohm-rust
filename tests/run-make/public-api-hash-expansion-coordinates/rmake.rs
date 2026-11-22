//@ needs-target-std

use run_make_support::{rfs, rustc};

const LIB: &str = r#"
pub async fn wait() {
    let _prefix = 12345;
    std::future::pending::<()>().await;
}

#[derive(Clone, Debug)]
pub struct Row(pub u8);
"#;

fn check(source: &str, dir: &str) -> Vec<u8> {
    rfs::write("dep.rs", source);
    rustc()
        .input("dep.rs")
        .crate_type("lib")
        .edition("2021")
        .emit("metadata")
        .arg("-Zpublic-api-hash")
        .arg(format!("-Cincremental={dir}"))
        .run();
    rustc().input("libdep.rmeta").arg("-Zls=public_hash").run().stdout()
}

fn main() {
    let original = check(LIB, "warm");
    // The await and the derive move in bytes while keeping their line and column.
    let short = LIB.replace("12345", "1");
    assert_eq!(original, check(&short, "warm"));
    assert_eq!(original, check(&short, "cold"));

    let moved = short.replace("std::future", "  std::future");
    let changed = check(&moved, "warm");
    assert_ne!(original, changed);
    assert_eq!(changed, check(&moved, "moved"));

    let moved_derive = short.replace("#[derive", "  #[derive");
    let changed = check(&moved_derive, "warm");
    assert_ne!(original, changed);
    assert_eq!(changed, check(&moved_derive, "moved-derive"));

    // A reader can evaluate Location::caller in a const fn. Its character column
    // can move even when every byte offset and line number stays the same.
    let ascii = "/*ab*/pub const fn col() -> u32 { std::panic::Location::caller().column() }";
    let utf8 = ascii.replace("ab", "é");
    assert_eq!(ascii.len(), utf8.len());
    let original = check(ascii, "columns");
    let changed = check(&utf8, "columns");
    assert_ne!(original, changed);
    assert_eq!(changed, check(&utf8, "columns-cold"));
    assert_eq!(original, check(ascii, "columns"));

    // Multibyte characters on earlier lines do not affect this column.
    let previous = ascii.replace("*/", "*/\n");
    let original = check(&previous, "previous-line");
    assert_eq!(original, check(&previous.replace("ab", "é"), "previous-line"));
}
