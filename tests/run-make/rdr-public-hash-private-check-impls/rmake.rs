//@ ignore-cross-compile

use run_make_support::{Rustc, rfs, rustc};

fn check_hash(exposed: bool) -> String {
    let mut cmd = rustc();
    cmd.input("dep.rs").incremental(if exposed { "exposed" } else { "private" });
    if exposed {
        cmd.arg("--cfg=exposed");
    }
    cmd.arg("-Zpublic-api-hash").emit("metadata").run();
    rustc().input("libdep.rmeta").arg("-Zls=public_hash").run().stdout_utf8()
}

fn consumer() -> Rustc {
    let mut cmd = rustc();
    cmd.input("consumer.rs").extern_("dep", "libdep.rmeta").emit("metadata");
    cmd
}

fn main() {
    let original = rfs::read_to_string("hidden.rs");
    let private = check_hash(false);
    let exposed = check_hash(true);

    // Handle's field reaches Guard for layout. Only guard() exposes it through an opaque.
    let method = original.replace("-> u16", "-> u32");
    rfs::write("hidden.rs", &method);
    assert_eq!(private, check_hash(false));
    let edited = check_hash(true);
    assert_ne!(exposed, edited);
    consumer().run();

    // State has both a derived and a handwritten impl. Returning it from state()
    // makes its field observable through the opaque's auto traits.
    rfs::write("hidden.rs", method.replace("&'a usize", "&'a std::cell::Cell<usize>"));
    assert_eq!(private, check_hash(false));
    assert_ne!(edited, check_hash(true));
    consumer().run_fail().assert_stderr_contains("cannot be shared between threads safely");

    // Lookup still observes the full From header even while State stays private.
    rfs::write("hidden.rs", original.replace("u8", "i8"));
    assert_ne!(private, check_hash(false));
}
