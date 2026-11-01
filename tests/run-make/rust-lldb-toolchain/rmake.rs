//@ only-unix

use run_make_support::{cmd, path, rfs, source_root};

fn main() {
    for name in ["quoted \"toolchain\"", "toolchain with a trailing newline\n"] {
        let toolchain = path(name);
        let bin = toolchain.join("bin");
        let rustc = bin.join("rustc");
        let lldb = toolchain.join("lib/rustlib/mock-host/bin/lldb");
        let launcher = bin.join("rust-lldb");
        rfs::create_dir_all(&bin);
        rfs::create_dir_all(lldb.parent().unwrap());
        rfs::copy(source_root().join("src/etc/rust-lldb"), &launcher);
        rfs::write(
            &rustc,
            r#"#!/bin/sh
case "$2" in
host-tuple) echo mock-host ;;
sysroot) dirname "$(dirname "$0")" ;;
esac
"#,
        );
        rfs::write(&lldb, "#!/bin/sh\nprintf '%s\\n' \"$_RUST_LLDB_PYTHON_DIR\" \"$@\"\n");
        cmd("chmod").arg("+x").arg(&rustc).arg(&lldb).run();

        let formatters = toolchain.join("lib/rustlib/etc");
        let output = cmd("sh").arg(launcher).arg("--batch").run();
        output.assert_stdout_contains(format!("{}\n--one-line-before-file", formatters.display()));
        output.assert_stdout_contains("command script import lldb_lookup\n--batch");
    }
}
