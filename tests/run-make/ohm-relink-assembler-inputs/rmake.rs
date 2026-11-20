//@ only-linux
//@ only-x86_64
//@ ignore-cross-compile
// Reason: the test executes x86-64 inline assembly on the host.

use run_make_support::{rfs, run, rustc};

fn main() {
    const FLAG: &str = "-Zohm-check-relink-inputs";
    const MARKER: &str = "ohm-untracked-relink-inputs";
    rustc().input("engine.rs").crate_type("rlib").emit("link,dep-info=engine.d").arg(FLAG).run();
    // The library defines the generic function, but only its caller emits the assembly.
    assert!(!rfs::read_to_string("engine.d").contains(MARKER));

    let compile_reader = || {
        let mut cmd = rustc();
        cmd.input("reader.rs")
            .edition("2021")
            .extern_("engine", "libengine.rlib")
            .arg("-Zemit-rlink")
            .arg(FLAG)
            .arg("-Cincremental=incr")
            .codegen_units(1)
            .emit("link,dep-info=reader.d");
        cmd
    };
    for byte in [17, 29] {
        rfs::write("payload.bin", [byte]);
        compile_reader().run();
        run("reader").assert_stdout_equals(byte.to_string());
        assert!(rfs::read_to_string("reader.d").contains(MARKER));
    }
    rfs::remove_file("payload.bin");
    compile_reader().run_fail().assert_stderr_contains("payload.bin");
}
