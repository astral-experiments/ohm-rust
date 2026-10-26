//@ ignore-cross-compile

use run_make_support::{rfs, rustc};

fn check_hash() -> String {
    rustc().input("dep.rs").arg("-Zpublic-api-hash").emit("metadata").run();
    rustc().arg("-Zls=public_hash").input("libdep.rmeta").run().stdout_utf8()
}

fn main() {
    let original = rfs::read_to_string("hidden.rs");
    let moved = original.replace("    fn internal", "\n    fn internal");
    let baseline = check_hash();
    rfs::write("hidden.rs", &moved);
    assert_eq!(baseline, check_hash());

    // The lookup index still distinguishes concrete trait arguments for a private self type.
    rfs::write(
        "hidden.rs",
        original.replace("From<u8>", "From<i8>").replace("from(_: u8)", "from(_: i8)"),
    );
    assert_ne!(baseline, check_hash());

    // Exposing the opaque interface retains the conservative impl graph.
    let mut root = rfs::read_to_string("dep.rs");
    root.push_str("pub fn opaque() -> impl Copy { hidden::Hidden }\n");
    rfs::write("dep.rs", root);
    rfs::write("hidden.rs", original);
    let exposed = check_hash();
    rfs::write("hidden.rs", moved);
    assert_ne!(exposed, check_hash());
}
