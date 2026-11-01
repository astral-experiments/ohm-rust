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
        .arg(&launcher)
        .env("TEST_SYSROOT", "Rust \"quoted\"\\install\n")
        .env("RUST_GDB", "./mock-gdb")
        .run();
    let escaped = "Rust \\\"quoted\\\"\\\\install\n";
    let rule = format!("set substitute-path /rustc/abc \"{escaped}/lib/rustlib/src/rust\"");
    output.assert_stdout_contains(rule);

    let toolchain = path("toolchain:with:colons");
    let etc = toolchain.join("lib/rustlib/etc");
    rfs::create_dir_all(&etc);
    let loader = "gdb_load_rust_pretty_printers.py";
    rfs::copy(source_root().join("src/etc").join(loader), etc.join(loader));
    rfs::write(
        "gdb.py",
        r#"import os
def execute(command, to_string=False):
    name, _, directory = command.rpartition(" ")
    expected = os.path.join(os.environ["TEST_SYSROOT"], "lib", "rustlib", "etc")
    assert ":" not in directory and os.path.samefile(directory, expected)
    print(name)
"#,
    );
    rfs::write(
        "mock-gdb",
        r#"#!/bin/sh
if [ "$1" != -ix ]; then
    echo "missing GDB formatter directory" >&2
    exit 1
fi
PYTHONPATH=. exec "$PYTHON" "$2"
"#,
    );
    cmd("sh")
        .arg(launcher)
        .env("TEST_SYSROOT", toolchain)
        .env("RUST_GDB", "./mock-gdb")
        .run()
        .assert_stdout_contains("directory\nadd-auto-load-safe-path");
}
