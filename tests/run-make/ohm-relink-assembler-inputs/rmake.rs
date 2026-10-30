//@ only-linux
//@ only-x86_64
//@ ignore-cross-compile
// Reason: the test executes x86-64 inline assembly on the host.

use run_make_support::{llvm_profdata, rfs, run, rustc};

fn main() {
    const FLAG: &str = "-Zohm-check-relink-inputs";
    const MARKER: &str = "ohm-untracked-relink-inputs";
    rustc().input("engine.rs").crate_type("rlib").emit("link,dep-info=engine.d").arg(FLAG).run();
    // The library defines the generic function, but only its caller emits the assembly.
    assert!(!rfs::read_to_string("engine.d").contains(MARKER));

    let compile_reader = |units| {
        let mut cmd = rustc();
        cmd.input("reader.rs")
            .edition("2021")
            .extern_("engine", "libengine.rlib")
            .arg("-Zemit-rlink")
            .arg(FLAG)
            .incremental(format!("incr-{units}"))
            .codegen_units(units)
            .emit("link,dep-info=reader.d");
        cmd
    };
    rfs::write("indirect.s", ".include \"second.s\"\n");
    for (direct, nested) in [(17, 1), (29, 1), (29, 2)] {
        rfs::write("payload.bin", [direct]);
        rfs::write("second.s", format!(".byte {nested}\n"));
        compile_reader(1).run();
        run("reader").assert_stdout_equals(u16::from_le_bytes([direct, nested]).to_string());
        assert!(rfs::read_to_string("reader.d").contains(MARKER));
    }
    rfs::remove_file("second.s");
    compile_reader(1).run_fail().assert_stderr_contains("second.s");
    rfs::write("second.s", ".byte 2\n");
    rfs::remove_file("payload.bin");
    compile_reader(1).run_fail().assert_stderr_contains("payload.bin");
    rfs::write("payload.bin", [17]);
    compile_reader(1).run();
    run("reader").assert_stdout_equals(u16::from_le_bytes([17, 2]).to_string());

    // The dependency's generic brings file-reading assembly into one of the reader's units.
    // Without LTO the independent object files can survive even a native-only edit.
    let compile_split = || {
        let mut cmd = compile_reader(16);
        cmd.arg("-Clto=off").arg("-Zquery-dep-graph").arg("-Zprint-mono-items");
        cmd
    };
    compile_split().run();
    rfs::write("payload.bin", [29]);
    compile_split()
        .arg("-Zassert-incr-state=loaded")
        .run()
        .assert_stdout_contains_regex(r"(?m)^CGU_REUSE .+ PostLto$")
        .assert_stdout_contains_regex(r"(?m)^CGU_REUSE .+ No$");
    run("reader").assert_stdout_equals(u16::from_le_bytes([29, 2]).to_string());
    assert!(rfs::read_to_string("reader.d").contains(MARKER));
    rfs::remove_file("payload.bin");
    compile_split().run_fail().assert_stderr_contains("payload.bin");

    // Every module can depend on an external profile, an ABI list, or the native host CPU.
    rfs::write("payload.bin", [29]);
    rfs::write("profile.txt", ":ir\narbitrary_function\n0\n1\n1\n");
    llvm_profdata().merge().output("merged.profdata").input("profile.txt").run();
    rfs::write("sample.prof", "arbitrary_function:1:1\n 1: 1\n");
    rfs::write("abi.list", "fun:arbitrary_function=uninstrumented\n");
    for option in [
        "-Cprofile-use=merged.profdata",
        "-Zprofile-sample-use=sample.prof",
        "-Zsanitizer-dataflow-abilist=abi.list",
        "-Ctarget-cpu=native",
        "-Ztune-cpu=native",
    ] {
        compile_split().arg(option).run();
        compile_split()
            .arg(option)
            .arg("-Zassert-incr-state=loaded")
            .run()
            .assert_stdout_contains_regex(r"(?m)^CGU_REUSE .+ No$")
            .assert_stdout_not_contains_regex(r"(?m)^CGU_REUSE .+ (PreLto|PostLto)$");
    }
}
