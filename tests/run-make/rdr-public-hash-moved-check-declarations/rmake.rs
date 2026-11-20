//@ ignore-cross-compile

use run_make_support::{rfs, rustc, serde_json};

const SOURCE: &str = "fn private() {}\n\
/// Add ink.\n\
pub fn write(pen: &str) -> &str { pen }\n\
pub fn copy<T: Copy>(input: T) -> T { input }\n";

fn hash(source: &str, hint: bool, native: bool) -> String {
    rfs::write("dep.rs", source);
    let mut cmd = rustc();
    cmd.input("dep.rs")
        .crate_type("rlib")
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich")
        .incremental("producer-cache")
        .json("artifacts")
        .error_format("json");
    if hint {
        cmd.arg("-Zohm-terminal-check-hash");
    }
    let output = cmd.emit(if native { "metadata,link" } else { "metadata" }).run();
    let message = output
        .stderr_utf8()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|msg| msg["$message_type"] == "artifact" && msg["emit"] == "metadata")
        .unwrap();
    message["public_api_hash"]["value"].as_str().unwrap().to_owned()
}

fn disk_hash() -> String {
    rustc().input("libdep.rmeta").arg("-Zls=public_hash").run().stdout_utf8()
}

fn reader() {
    rfs::write("reader.rs", "fn main() { dep::write(dep::copy(\"x\")); }\n");
    rustc()
        .input("reader.rs")
        .emit("metadata")
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich")
        .incremental("reader-cache")
        .extern_("dep", "libdep.rmeta")
        .run();
}

fn main() {
    let moved = SOURCE.replace("private() {}", "private() {\n    let _ = 0;\n}");
    let first = hash(SOURCE, true, false);
    let disk = disk_hash();
    reader();
    assert_eq!(first, hash(&moved, true, false));
    assert_ne!(disk, disk_hash());
    reader();

    // A reader invoked after the edit should see the current declaration coordinates.
    rfs::write("reader.rs", "fn main() { dep::write(); }\n");
    rustc()
        .input("reader.rs")
        .emit("metadata")
        .extern_("dep", "libdep.rmeta")
        .run_fail()
        .assert_stderr_contains("dep.rs:5:8");
    let warm = rfs::read("libdep.rmeta");
    rfs::remove_dir_all("producer-cache");
    assert_eq!(first, hash(&moved, true, false));
    assert_eq!(warm, rfs::read("libdep.rmeta"));
    let disk = disk_hash();
    let edited = moved.replace("= 0", "= 1");
    assert_eq!(first, hash(&edited, true, false));
    assert_ne!(disk, disk_hash());
    assert_ne!(first, hash(&edited, false, false));
    assert_eq!(first, hash(&edited, true, false));

    for changed in [
        SOURCE.replace("pen", "ink"),
        SOURCE.replace("Add ink", "New ink"),
        SOURCE.replace("&str", "&u32"),
        SOURCE.replace("write", "rewrite"),
        SOURCE.replace("write", "r#write"),
        SOURCE.replace("T: Copy", "T: Send"),
    ] {
        assert_ne!(first, hash(&changed, true, false), "{changed}");
    }
    for observable in [
        "#[track_caller] pub fn location() {}",
        "pub const LINE: u32 = line!();",
        "#[macro_export] macro_rules! number { () => { 5 } }",
        "#[doc = \"Add ink.\"] pub fn ink() {}",
    ] {
        assert_ne!(
            hash(&format!("{SOURCE}{observable}"), true, false),
            hash(&format!("{moved}{observable}"), true, false),
            "{observable}",
        );
    }
    assert_ne!(hash(SOURCE, true, true), hash(&moved, true, true));
}
