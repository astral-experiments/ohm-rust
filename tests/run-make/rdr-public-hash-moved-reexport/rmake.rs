//@ ignore-cross-compile

use run_make_support::{Rustc, rfs, rustc, serde_json};

fn check(name: &str) -> Rustc {
    let mut cmd = rustc();
    cmd.input(format!("{name}.rs"))
        .edition("2024")
        .incremental(format!("{name}-cache"))
        .emit("metadata")
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich");
    cmd
}

fn rebuild(source: &str) -> (String, String) {
    rfs::write("producer.rs", source);
    check("producer").crate_type("rlib").run();
    let exact = rustc().input("libproducer.rmeta").arg("-Zls=public_hash").run().stdout_utf8();
    let output = check("facade")
        .crate_type("rlib")
        .extern_("producer", "libproducer.rmeta")
        .arg("-Zohm-terminal-check-hash")
        .arg("-Zohm-transitive-check-hash")
        .json("artifacts")
        .error_format("json")
        .run();
    let artifact = output
        .stderr_utf8()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|msg| msg["$message_type"] == "artifact" && msg["emit"] == "metadata")
        .unwrap();
    (exact, artifact["public_api_hash"]["value"].as_str().unwrap().to_owned())
}

fn main() {
    rfs::write("facade.rs", "pub use producer::{Key, read};\n");
    let client = || {
        let mut cmd = check("client");
        cmd.extern_("facade", "libfacade.rmeta");
        cmd
    };
    let caller = "fn main() { facade::read(); let _: facade::Key = 0u8.into(); }\n";
    rfs::write("client.rs", caller);
    let original = "pub struct Key;\n\
                    impl From<u8> for Key { fn from(_: u8) -> Self { Self } }\n\
                    /// Read a key.\n\
                    pub fn /*aa*/read() {}\n";
    let (mut exact, hint) = rebuild(original);
    client().run();

    // Adding a doc line moves the declaration. The equal-byte edit on the next
    // pass moves its character column without changing any byte offsets.
    let moved = original.replace("pub fn", "/// Returns immediately.\npub fn");
    let unicode = moved.replace("/*aa*/", "/*é*/");
    for (source, location) in [(&moved, "producer.rs:5:14"), (&unicode, "producer.rs:5:13")] {
        let (next, projected) = rebuild(source);
        assert_ne!(exact, next);
        assert_eq!(hint, projected);
        exact = next;

        // After a skipped check, an edit to the binary must see the new coordinates.
        rfs::write("client.rs", "fn main() { facade::read(1); }\n");
        client().run_fail().assert_stderr_contains("E0061").assert_stderr_contains(location);
        rfs::write("client.rs", caller);
        client().run();
    }

    // The reexport is unchanged, but the impl used by the binary no longer applies.
    let (_, changed) = rebuild(&unicode.replace("u8", "u16"));
    assert_ne!(hint, changed);
    client().run_fail().assert_stderr_contains("E0277");

    // Swapping identically named definitions preserves the set of locations.
    // The exact hash must still associate each location with its owner.
    let a = "mod a { pub struct S; }\n";
    let b = "mod b { pub struct S; }\n";
    let exports = "pub use a::S as Key; pub use b::S as read;\n";
    let (first, _) = rebuild(&format!("{a}{b}{exports}"));
    let (swapped, _) = rebuild(&format!("{b}{a}{exports}"));
    assert_ne!(first, swapped);
}
