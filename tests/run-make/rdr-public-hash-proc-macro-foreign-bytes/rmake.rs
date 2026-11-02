//@ ignore-cross-compile

use std::path::Path;

use run_make_support::{dynamic_lib_name, rfs, run, rust_lib_name, rustc};

fn leaf_hash() -> String {
    rustc()
        .input("leaf.rs")
        .crate_type("rlib")
        .incremental("leaf-incr")
        .arg("-Zpublic-api-hash")
        .run();
    rustc().input(rust_lib_name("leaf")).arg("-Zls=public_hash").run().stdout_utf8()
}

fn client(mode: &str, crate_type: &str, expects_marker: bool) {
    rustc()
        .input("client.rs")
        .edition("2021")
        .crate_type(crate_type)
        .arg(format!("--cfg={mode}"))
        .extern_("probe", dynamic_lib_name("probe"))
        .extern_("leaf", rust_lib_name("leaf"))
        .emit("dep-info=actual.d,link")
        .arg("-Zpublic-api-hash")
        .run();
    let deps = rfs::read_to_string("actual.d");
    let marker = Path::new("actual.d").join("ohm-foreign-span-bytes");
    let inputs = deps.lines().find_map(|line| line.strip_prefix("actual.d: ")).unwrap();
    assert_eq!(inputs.split_whitespace().any(|p| Path::new(p) == marker), expects_marker, "{deps}");
    if mode == "text" {
        assert!(inputs.split_whitespace().any(|p| Path::new(p).ends_with("output.rs")), "{deps}");
    }
}

fn observed_byte_start() -> usize {
    client("observer", "bin", true);
    let output = run("client").stdout_utf8();
    let (_, bytes) = output.trim().split_once("bytes(").unwrap();
    bytes.split_once("..").unwrap().0.parse().unwrap()
}

fn main() {
    rustc().input("probe.rs").crate_type("proc-macro").run();
    let original = leaf_hash();
    client("clone_only", "bin", false);
    client("local", "bin", false);
    let before = observed_byte_start();
    // Reading a foreign position in a downstream library must also guard its depinfo.
    client("observer", "rlib", true);

    // The first callback imports before.rs without inspecting its input. The next callback
    // sees the same token in output.rs, whose absolute byte position follows the first file.
    let source = rfs::read_to_string("before.rs");
    rfs::write("before.rs", source.replace("Private change", "Private changes"));
    assert_eq!(original, leaf_hash());
    assert_eq!(observed_byte_start(), before + 1);

    // source_text authenticates the complete original file, including comments after the macro.
    client("text", "bin", true);
    run("client").assert_stdout_equals("Some(\"foreign_marker\")\n");
    let output = rfs::read_to_string("output.rs");
    rfs::write("output.rs", output.replace("Unrelated note", "Unrelated edit"));
    assert_eq!(original, leaf_hash());
    client("text", "bin", true);
    run("client").assert_stdout_equals("Some(\"foreign_marker\")\n");

    // Ordinary dependency metadata already tracks the complete producer hash.
    rustc().input("leaf.rs").crate_type("rlib").run();
    client("observer", "rlib", false);
}
