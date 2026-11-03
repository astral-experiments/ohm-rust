//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn set_asset(len: usize, middle: u8) {
    let mut bytes = vec![b'a'; len];
    bytes[len / 2] = middle;
    rfs::write("asset", bytes);
}

fn check(history: &str, expected: &str) {
    rustc().input("lib.rs").incremental(format!("{history}-lib")).run();
    rustc()
        .input("main.rs")
        .extern_("producer", "libproducer.rlib")
        .incremental(format!("{history}-main"))
        .run();
    run("main").assert_stdout_equals(expected);
}

fn main() {
    for len in [65_535, 65_536, 65_537] {
        set_asset(len, b'a');
        check("warm", "97 19 0\n");
    }
    set_asset(65_537, b'b');
    check("warm", "98 83 0\n");
    check("empty", "98 83 0\n");

    // The raw bytes are valid, but the evaluated constant now contains uninitialized data.
    set_asset(65_537, b'!');
    rustc().input("lib.rs").incremental("warm-lib").run_fail().assert_stderr_contains("E0080");
    set_asset(65_537, b'a');
    check("warm", "97 19 0\n");
}
