//@ ignore-cross-compile

use run_make_support::{rfs, rustc};

fn main() {
    // Formatting changes the end of the declaration without changing its public hash.
    // Editing the reader at the same time makes it look up the imported span again.
    let mut hashes = Vec::new();
    for (library, reader) in [
        ("pub fn value()->u32{42}", "fn main(){let _=input::value();}"),
        ("pub fn value() -> u32 {\n    42\n}", "fn main() {\n    let _ = input::value();\n}"),
    ] {
        rfs::write("input.rs", library);
        rustc()
            .input("input.rs")
            .crate_type("lib")
            .emit("metadata")
            .arg("-Zpublic-api-hash")
            .incremental("library-cache")
            .run();
        hashes.push(rustc().arg("-Zls=public_hash").input("libinput.rmeta").run().stdout_utf8());

        rfs::write("reader.rs", reader);
        rustc()
            .input("reader.rs")
            .edition("2024")
            .extern_("input", "libinput.rmeta")
            .emit("metadata")
            .arg("-Zpublic-api-hash")
            .incremental("reader-cache")
            .run();
    }
    assert_eq!(hashes[0], hashes[1]);
}
