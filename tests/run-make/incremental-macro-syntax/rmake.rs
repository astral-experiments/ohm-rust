//@ needs-target-std

use run_make_support::{Rustc, rfs, rustc};

fn dependency(result: &str) {
    let source = format!(
        r#"
        #[macro_export]
        macro_rules! inner {{ ($n:ident, $value:expr) => {{ {result} }} }}
        #[macro_export]
        macro_rules! outer {{ ($arg:expr) => {{{{ let x = 2; $crate::inner!(x, $arg) }}}} }}
        #[macro_export]
        macro_rules! fallback {{
            ($expr:expr; required) => {{ 1 }};
            ($($other:tt)*) => {{ 2 }};
        }}
        "#
    );
    rfs::write("dep.rs", source);
    rustc().input("dep.rs").edition("2024").crate_type("rlib").run();
}

fn check(cache: &str) -> Rustc {
    let mut cmd = rustc();
    cmd.input("lib.rs")
        .edition("2024")
        .crate_type("rlib")
        .arg("--extern=dep=libdep.rlib")
        .emit("metadata")
        .incremental(cache)
        .arg("-Zohm-shared-stages")
        .arg("-Zincremental-verify-ich")
        .env("RUSTC_LOG", "ohm_macro_syntax=info");
    cmd
}

fn main() {
    dependency("1 + $n + $value");
    rfs::write(
        "calls.rs",
        r#"
        pub fn f(value: u32) -> u32 {
            dep::outer!(value + 1) + dep::fallback!(try { 1 })
        }
        "#,
    );
    // The unsuccessful first arm of fallback! discards the try-block gate.
    // Moving the caller file in the source map also exercises span relocation.
    for (index, prefix) in ["", "// longer\n", ""].into_iter().enumerate() {
        rfs::write("lib.rs", format!("{prefix}pub mod calls;"));
        let output = check("cache").run();
        if index > 0 {
            output.assert_stderr_contains("reused macro expansion");
        }
    }

    // Combining spans from the caller and the macro definition reads entries
    // in the metavar map. The ordinary crate hash includes these reads.
    check("reads").arg("-Zmetadata-crate-hash=no").run();
    let expected = rfs::read("liblib.rmeta");
    check("reads")
        .arg("-Zmetadata-crate-hash=no")
        .run()
        .assert_stderr_contains("reused macro expansion");
    assert!(rfs::read("liblib.rmeta") == expected);

    // A change to the imported rule must reach the unchanged caller.
    dependency("false");
    check("cache").run_fail().assert_stderr_contains("cannot add");
    check("clean").run_fail().assert_stderr_contains("cannot add");
}
