//@ needs-target-std

use run_make_support::{rfs, rustc, rustdoc};

fn main() {
    rustc().input("origin.rs").crate_type("rlib").emit("metadata").arg("-Zpublic-api-hash").run();

    rustdoc()
        .input("client.rs")
        .edition("2024")
        .extern_("origin", "liborigin.rmeta")
        .out_dir("doc")
        .run();
    let marker = rfs::read_to_string("doc/client/struct.Marker.html");
    assert!(marker.contains("method.describe"), "missing blanket implementation of Describe");

    rustc()
        .input("client.rs")
        .edition("2024")
        .crate_type("rlib")
        .emit("metadata")
        .extern_("origin", "liborigin.rmeta")
        .arg("-Zpublic-api-hash")
        .run_fail()
        .assert_stderr_contains("use origin::Describe;");
}
