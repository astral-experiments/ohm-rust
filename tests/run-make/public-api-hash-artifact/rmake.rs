//@ needs-target-std

use run_make_support::{rfs, rustc, serde_json};

fn compile_and_read_public_api_hash() -> String {
    let output = rustc()
        .input("lib.rs")
        .crate_type("rlib")
        .emit("metadata")
        .arg("-Zpublic-api-hash")
        .json("artifacts")
        .error_format("json")
        .incremental("incremental")
        .run();

    let notification = output
        .stderr_utf8()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|message| message["$message_type"] == "artifact" && message["emit"] == "metadata")
        .expect("rustc did not emit a metadata artifact notification");
    let public_api_hash = &notification["public_api_hash"];
    assert_eq!(public_api_hash["version"], 1);

    let value = public_api_hash["value"].as_str().expect("public API hash value was not a string");
    assert_eq!(value.len(), 32);
    assert!(
        value.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
        "public API hash was not lowercase hexadecimal: {value}",
    );
    value.to_owned()
}

fn main() {
    let fresh_hash = compile_and_read_public_api_hash();

    let private_source = rfs::read_to_string("private.rs");
    let changed_private_source = private_source.replacen("    1", "    2", 1);
    assert_ne!(private_source, changed_private_source);
    rfs::write("private.rs", changed_private_source);

    // This private-body edit leaves the metadata dep-node green, so the second compilation reuses
    // its work product. The notification must recover the hash from that copied rmeta rather than
    // relying on the fresh metadata encoder.
    let reused_hash = compile_and_read_public_api_hash();
    assert_eq!(fresh_hash, reused_hash);
}
