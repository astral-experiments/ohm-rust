//@ ignore-cross-compile

use std::fs;

use run_make_support::rustc;

fn build(cache: &str, error: bool) -> String {
    let mut cmd = rustc();
    cmd.input("input.rs")
        .arg("--edition=2024")
        .arg("--error-format=json")
        .arg("--emit=metadata=libqpath_sample.rmeta")
        .arg("--crate-name=qpath_sample")
        .arg("--crate-type=rlib")
        .arg("-Cmetadata=qpath_geometry")
        .arg(format!("-Cincremental={cache}"))
        .arg("-Zincremental-verify-ich")
        .arg("-L.")
        .arg("--extern=qpath_macros")
        .arg("--extern=qpath_support");
    let output = if error { cmd.run_fail() } else { cmd.run() };
    output.stderr_utf8()
}

fn main() {
    rustc().input("qpath_support.rs").arg("--edition=2024").arg("--crate-type=rlib").run();
    rustc().input("qpath_macros.rs").arg("--edition=2024").arg("--crate-type=proc-macro").run();

    let original = fs::read_to_string("client.rs").unwrap();
    let included = fs::read_to_string("included.rs.in").unwrap();
    fs::write("input.rs", &original).unwrap();
    fs::write("included.rs", &included).unwrap();
    build("incremental", false);

    let shifted = format!("\n\n\n\n\n{included}");
    fs::write("included.rs", shifted).unwrap();
    let changed = build("incremental", false);
    let warm_metadata = fs::read("libqpath_sample.rmeta").unwrap();
    assert_eq!(changed, build("empty-success", false));
    assert_eq!(warm_metadata, fs::read("libqpath_sample.rmeta").unwrap());
    assert!(changed.contains("qualified variant geometry"));

    let erroneous = original.replace("Self::Node { value: 8 }", "Self::Node {}");
    assert_ne!(erroneous, original);
    fs::write("input.rs", erroneous).unwrap();
    let warm_error = build("incremental", true);
    assert_eq!(warm_error, build("empty-error", true));
    assert!(warm_error.contains("E0063"));
}
