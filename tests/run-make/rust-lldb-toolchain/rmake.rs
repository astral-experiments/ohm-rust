//@ only-unix

use run_make_support::{cmd, path, rfs, source_root};

fn main() {
    let toolchain = path("toolchain with spaces");
    let rustc = toolchain.join("rustc");
    let lldb = toolchain.join("lib/rustlib/mock-host/bin/lldb");
    let launcher = toolchain.join("rust-lldb");
    rfs::create_dir_all(lldb.parent().unwrap());
    rfs::copy(source_root().join("src/etc/rust-lldb"), &launcher);
    rfs::write(
        &rustc,
        r#"#!/bin/sh
case "$2" in
host-tuple) echo mock-host ;;
sysroot) dirname "$0" ;;
esac
"#,
    );
    rfs::write(&lldb, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n");
    cmd("chmod").arg("+x").arg(&rustc).arg(&lldb).run();

    let formatters = toolchain.join("lib/rustlib/etc/lldb_lookup.py");
    cmd("sh").arg(launcher).arg("--batch").run().assert_stdout_equals(format!(
        "--one-line-before-file\ncommand script import \"{}\"\n--batch",
        formatters.display(),
    ));
}
