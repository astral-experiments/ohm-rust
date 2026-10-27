//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn library() -> String {
    rustc()
        .input("dep.rs")
        .incremental("incremental")
        .opt_level("1")
        .arg("-Zpublic-api-hash")
        .run();
    rustc().input("libdep.rlib").arg("-Zls=public_hash").run().stdout_utf8()
}

fn caller() {
    rustc()
        .input("main.rs")
        .extern_("dep", "libdep.rlib")
        .arg("-Zpublic-api-hash")
        .arg("-Zemit-rlink")
        .arg("-Ztemps-dir=objects")
        .run();
}

fn main() {
    let original = rfs::read_to_string("hidden.rs");
    let baseline = library();
    caller();
    run("main").assert_stdout_equals("1 1\n");

    // The caller instantiates Layout's size, but never selects its private method or From<Input>.
    let edited = original.replace("    fn adjusted", "\n    fn adjusted").replace("+ 1", "+ 2");
    rfs::write("hidden.rs", &edited);
    assert_eq!(baseline, library());
    rustc().input("main.rlink").arg("-Zemit-rlink").arg("-Zlink-only").run();
    run("main").assert_stdout_equals("1 2\n");

    // Changing that same private type's layout must invalidate the caller's generic code.
    rfs::write("hidden.rs", edited.replace("struct Layout(u8)", "struct Layout(u64)"));
    assert_ne!(baseline, library());
    caller();
    run("main").assert_stdout_equals("8 2\n");
}
