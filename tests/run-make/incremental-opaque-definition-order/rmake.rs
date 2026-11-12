//@ needs-target-std

use run_make_support::{rfs, rustc};

fn main() {
    // A private body edit keeps all the opaque definitions and their source positions unchanged.
    for value in [1, 2] {
        rfs::write("private.rs", format!("#[expect(dead_code)] fn value() -> u32 {{ {value} }}"));
        let output = rustc()
            .input("lib.rs")
            .edition("2024")
            .crate_type("rlib")
            .emit("metadata")
            .incremental("incremental")
            .arg("-Zpublic-api-hash")
            .arg("-Zthreads=8")
            .env("RUSTC_LOG", "ohm_metadata_reuse=info")
            .run();
        assert_eq!(output.stderr_utf8().contains("reused metadata item prefix"), value == 2);
    }
}
