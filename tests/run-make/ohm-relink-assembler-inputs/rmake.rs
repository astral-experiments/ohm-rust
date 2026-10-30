//@ only-linux
//@ only-x86_64
//@ ignore-cross-compile
// Reason: the test executes x86-64 inline assembly on the host.

use run_make_support::{cwd, rfs, run, rustc};

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
    rfs::write("indirect.s", ".include \"second.s\"\n");
    for (direct, nested) in [(17, 1), (29, 1), (29, 2)] {
        rfs::write("payload.bin", [direct]);
        rfs::write("second.s", format!(".byte {nested}\n"));
        compile_reader().run();
        run("reader").assert_stdout_equals(u16::from_le_bytes([direct, nested]).to_string());
        assert!(rfs::read_to_string("reader.d").contains(MARKER));
    }
    rfs::remove_file("second.s");
    compile_reader().run_fail().assert_stderr_contains("second.s");
    rfs::write("second.s", ".byte 2\n");
    rfs::remove_file("payload.bin");
    compile_reader().run_fail().assert_stderr_contains("payload.bin");
    rfs::write("payload.bin", [17]);
    compile_reader().run();
    run("reader").assert_stdout_equals(u16::from_le_bytes([17, 2]).to_string());

    check_branch_switch_without_native_input_checks();
}

// With native input checks disabled, returning to A must not resurrect its old assembler bytes.
fn check_branch_switch_without_native_input_checks() {
    rfs::create_dir_all("checkout/.git");
    let a = rfs::read_to_string("reader.rs");
    let b = "fn main() {}";
    let switch = |branch: &str, source: &str| {
        rfs::write("checkout/.git/HEAD", format!("ref: refs/heads/{branch}\n"));
        rfs::write("checkout/main.rs", source);
    };
    let compile = || {
        let mut cmd = rustc();
        cmd.input("checkout/main.rs")
            .arg("-Zohm-branch-cache=yes")
            .env("CARGO_MANIFEST_DIR", cwd().join("checkout"))
            .edition("2021")
            .extern_("engine", "libengine.rlib")
            .incremental("checkout-incr")
            .codegen_units(1)
            .output("branch-reader");
        cmd
    };

    rfs::write("second.s", ".byte 0\n");
    rfs::write("payload.bin", [17]);
    switch("a", &a);
    compile().run();
    run("branch-reader").assert_stdout_equals("17");
    switch("b", b);
    compile().run();
    rfs::write("payload.bin", [29]);
    switch("a", &a);
    compile().run();
    run("branch-reader").assert_stdout_equals("29");
    switch("b", b);
    compile().run();
    rfs::remove_file("payload.bin");
    switch("a", &a);
    compile().run_fail().assert_stderr_contains("payload.bin");
}
