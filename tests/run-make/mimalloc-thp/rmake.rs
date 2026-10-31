//@ only-linux
//@ ignore-cross-compile

use run_make_support::rustc;

fn main() {
    let output = rustc()
        .arg("--version")
        .env("MIMALLOC_VERBOSE", "1")
        .env_remove("MIMALLOC_ALLOW_THP")
        .env_remove("mimalloc_allow_thp")
        .run()
        .stderr_utf8();
    if !output.contains("mimalloc:") {
        return;
    }
    assert!(output.contains("option 'allow_thp': 0"), "{output}");

    rustc()
        .arg("--version")
        .env("MIMALLOC_VERBOSE", "1")
        .env("MIMALLOC_ALLOW_THP", "2")
        .run()
        .assert_stderr_contains("option 'allow_thp': 2");
}
