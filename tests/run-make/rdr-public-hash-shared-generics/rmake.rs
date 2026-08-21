//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn public_hash() -> String {
    rustc()
        .arg("-Zls=public_hash")
        .input("liba.rlib")
        .run()
        .stdout_utf8()
        .lines()
        .find_map(|line| line.strip_prefix("Public hash:"))
        .map(|hash| hash.trim().to_owned())
        .expect("public hash should be present")
}

fn compile_a(configuration: Option<&str>) {
    let mut command = rustc();
    command
        .input("a.rs")
        .incremental("partition-a")
        .arg("-Zpublic-api-hash")
        .extern_("shared", "libshared.rlib");
    if let Some(configuration) = configuration {
        command.cfg(configuration);
    }
    command.run();
}

fn compile_b() {
    rustc()
        .input("b.rs")
        .incremental("partition-b")
        .arg("-Zpublic-api-hash")
        .extern_("a", "liba.rlib")
        .extern_("shared", "libshared.rlib")
        .run();
}

fn main() {
    rustc().input("shared.rs").arg("-Zpublic-api-hash").run();

    compile_a(None);
    let original_hash = public_hash();
    compile_b();
    let original_b = rfs::read("libb.rlib");

    // A private body edit that does not change the exported generic set keeps the fast path.
    compile_a(Some("change_private_body"));
    assert_eq!(original_hash, public_hash());
    assert_eq!(original_b, rfs::read("libb.rlib"));
    rustc()
        .input("main.rs")
        .arg("-Zpublic-api-hash")
        .extern_("a", "liba.rlib")
        .extern_("b", "libb.rlib")
        .extern_("shared", "libshared.rlib")
        .run();
    run("main");

    // Removing an upstream generic provider must invalidate the stale dependent metadata.
    compile_a(Some("remove_shared"));
    assert_ne!(original_hash, public_hash());
    assert_eq!(original_b, rfs::read("libb.rlib"));
    rustc()
        .input("main.rs")
        .arg("-Zpublic-api-hash")
        .extern_("a", "liba.rlib")
        .extern_("b", "libb.rlib")
        .extern_("shared", "libshared.rlib")
        .run_fail();

    // Rebuilding the dependent against the changed export set restores a valid link.
    compile_b();
    rustc()
        .input("main.rs")
        .arg("-Zpublic-api-hash")
        .extern_("a", "liba.rlib")
        .extern_("b", "libb.rlib")
        .extern_("shared", "libshared.rlib")
        .run();
    run("main");
}
