//@ needs-target-std

use run_make_support::{Rustc, rfs, rustc};

fn check(cache: &str) -> Rustc {
    let mut cmd = rustc();
    cmd.input("lib.rs")
        .edition("2024")
        .crate_type("rlib")
        .emit("metadata")
        .incremental(cache)
        .arg("-Zincremental-verify-ich");
    cmd
}

fn main() {
    // Change the guard without moving the call, then move it and restore the guard.
    for (i, (prefix, bound, expected)) in
        [("", 1, true), ("", 2, false), ("\n", 1, true)].into_iter().enumerate()
    {
        rfs::write(
            "lib.rs",
            format!(
                "{prefix}{}",
                include_str!("input.rs")
                    .replace("BOUND", &bound.to_string())
                    .replace("EXPECTED", &expected.to_string())
            ),
        );
        check("warm").run();
        let warm = rfs::read("liblib.rmeta");
        // Disabling diagnostic deduplication selects the declarative expansion.
        check(&format!("fresh{i}")).arg("-Zdeduplicate-diagnostics=no").run();
        assert!(warm == rfs::read("liblib.rmeta"));
    }

    // Parsing can emit an error and still produce a pattern. Keep the matcher's diagnostics.
    rfs::write("lib.rs", "pub const BAD: bool = matches!(0, 0 | if 1);");
    check("warm")
        .run_fail()
        .assert_stderr_contains("a trailing `|` is not allowed")
        .assert_stderr_contains("expected pattern");
}
