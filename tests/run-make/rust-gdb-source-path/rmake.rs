//@ only-unix

use run_make_support::{cmd, path, rfs, source_root};

fn main() {
    let launcher = path("rust-gdb");
    rfs::copy(source_root().join("src/etc/rust-gdb"), &launcher);
    rfs::write(
        "rustc",
        r#"#!/bin/sh
case "$1" in
--print=sysroot) printf '%s\n' "$TEST_SYSROOT" ;;
-vV) echo 'commit-hash: abc' ;;
esac
"#,
    );
    rfs::write("mock-gdb", "#!/bin/sh\nprintf '%s\\n' \"$@\"\n");
    cmd("chmod").args(&["+x", "rustc", "mock-gdb"]).run();
    let output = cmd("sh")
        .arg(launcher)
        .env("TEST_SYSROOT", "Rust \"quoted\"\\install\n")
        .env("RUST_GDB", "./mock-gdb")
        .run();
    let escaped = "Rust \\\"quoted\\\"\\\\install\n";
    let rule = format!("set substitute-path /rustc/abc \"{escaped}/lib/rustlib/src/rust\"");
    output.assert_stdout_contains(rule);
}
