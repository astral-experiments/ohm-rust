//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn library_hash(exposed: bool) -> String {
    let mut cmd = rustc();
    cmd.input("dep.rs").incremental(if exposed { "exposed" } else { "private" }).opt_level("1");
    if exposed {
        cmd.arg("--cfg=exposed");
    }
    cmd.arg("-Zpublic-api-hash").run();
    rustc().input("libdep.rlib").arg("-Zls=public_hash").run().stdout_utf8()
}

fn check_consumer(size: &str) {
    rustc().input("consumer.rs").extern_("dep", "libdep.rlib").run();
    run("consumer").assert_stdout_equals(size);
}

fn main() {
    let path = "rules/hidden.rs";
    let original = rfs::read_to_string(path);
    let baseline = library_hash(false);
    // A macro and opaque return elsewhere in the crate expose only a primitive.
    let moved = original.replace("dataclasses.field", "dataclass.field");
    rfs::write(path, &moved);
    assert_eq!(baseline, library_hash(false));

    // The macro now returns the private type through a public opaque interface.
    let exposed = library_hash(true);
    check_consumer("1\n");
    rfs::write(path, moved.replace("u8", "u16"));
    assert_ne!(exposed, library_hash(true));
    check_consumer("2\n");
}
