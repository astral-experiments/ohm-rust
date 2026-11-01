//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn compile(cache_state: &str, expected: &str) -> String {
    rustc()
        .input("lib.rs")
        .crate_name("dep")
        .crate_type("rlib")
        .incremental("cache")
        .opt_level("1")
        .arg("-Zpublic-api-hash")
        .arg(format!("-Zassert-incr-state={cache_state}"))
        .run();
    rustc().input("main.rs").extern_("dep", "libdep.rlib").run();
    run("main").assert_stdout_equals(expected);
    rustc().arg("-Zls=public_hash").input("libdep.rlib").run().stdout_utf8()
}

fn main() {
    rfs::copy("right.rs", "left.rs");
    let before = compile("not-loaded", "Or Or true 17\n");

    // The unchanged derive reloads the same variant-name strings that the edited one creates.
    let edited = rfs::read_to_string("left.rs")
        .replace("FloorDiv,", "FloorDiv, LShift,")
        .replace("Operator::Or", "Operator::LShift");
    rfs::write("left.rs", edited);
    let warm = compile("loaded", "LShift Or true 17\n");
    rfs::remove_dir_all("cache");
    let empty = compile("not-loaded", "LShift Or true 17\n");

    // Equal bytes do not make two named static objects interchangeable.
    let switched = rfs::read_to_string("lib.rs")
        .replace("[&FIRST, &FIRST, &SECOND]", "[&FIRST, &SECOND, &SECOND]");
    rfs::write("lib.rs", switched);
    let changed_pointer = compile("loaded", "LShift Or false 17\n");
    rfs::remove_dir_all("cache");
    assert_eq!(changed_pointer, compile("not-loaded", "LShift Or false 17\n"));
    assert_ne!(empty, changed_pointer);
    assert_ne!(before, empty);
    assert_eq!(warm, empty, "cached CTFE literals changed the public hash");
}
