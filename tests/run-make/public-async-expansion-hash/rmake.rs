//@ needs-target-std

use run_make_support::{rfs, rustc};

const LIB: &str = r#"
pub async fn wait() {
    let _prefix = 12345;
    std::future::pending::<()>().await;
}
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
    // Both .await desugarings still have the same source range and text.
    let short = LIB.replace("12345", "1");
    assert_eq!(original, check(&short, "warm"));
    assert_eq!(original, check(&short, "cold"));

    let moved = short.replace("std::future", "  std::future");
    let changed = check(&moved, "warm");
    assert_ne!(original, changed);
    assert_eq!(changed, check(&moved, "moved"));
}
