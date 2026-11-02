//@ only-unix

use run_make_support::{cmd, path, python_command, rfs, source_root};

fn main() {
    let toolchain = path("Rust \"toolchain\\ folder");
    let bin = toolchain.join("bin");
    rfs::create_dir_all(&bin);
    let launcher = bin.join("rust-gdbgui");
    let rustc = bin.join("rustc");
    rfs::copy(source_root().join("src/etc/rust-gdbgui"), &launcher);
    rfs::write(
        &rustc,
        r#"#!/bin/sh
case "$1" in
--print=sysroot) printf '%s\n' "$TEST_SYSROOT" ;;
-vV) echo 'commit-hash: abc' ;;
esac
"#,
    );
    rfs::write("mock-frontend", "#!/bin/sh\nprintf '%s\\000' \"$@\" > gui-args\n");
    cmd("chmod").arg("+x").arg(&rustc).arg("mock-frontend").run();
    cmd("sh")
        .arg(launcher)
        .arg("program with spaces")
        .env("TEST_SYSROOT", &toolchain)
        .env("RUST_GDB", "\"custom gdb\" --nx")
        .env("RUST_GDBGUI", "./mock-frontend")
        .run();

    python_command()
        .arg("-c")
        .arg(
            r#"import os, shlex, sys
from pathlib import Path
args = [os.fsdecode(a) for a in Path("gui-args").read_bytes().split(b"\0")[:-1]]
assert args[0] == "--gdb-cmd" and args[2:] == ["program with spaces"], args
command = shlex.split(args[1])
assert command[:2] == ["custom gdb", "--nx"], command
assert "--directory=" + sys.argv[1] + "/lib/rustlib/etc" in command, command
"#,
        )
        .arg(&toolchain)
        .run();
}
