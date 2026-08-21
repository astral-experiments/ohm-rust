//@ ignore-cross-compile

use run_make_support::{rfs, rustc};

fn compile_and_read_public_hash() -> String {
    rustc().input("dep.rs").arg("-Zpublic-api-hash").incremental("incremental").run();

    let listing = rustc().arg("-Zls=public_hash").input("libdep.rlib").run().stdout_utf8();

    listing
        .lines()
        .find_map(|line| line.strip_prefix("Public hash:"))
        .map(|hash| hash.trim().to_owned())
        .expect("`-Zls=public_hash` did not print a `Public hash:` line")
}

fn main() {
    let baseline_hash = compile_and_read_public_hash();

    let source = rfs::read_to_string("dep.rs");
    let moved_source = source.replace(
        "#[track_caller]\npub fn",
        "#[track_caller]\n// Move only the definition start.\n//\n//\n//\n//\npub fn",
    );
    assert_ne!(source, moved_source, "track-caller definition was not moved");
    rfs::write("dep.rs", moved_source);

    let moved_hash = compile_and_read_public_hash();
    assert_ne!(
        baseline_hash, moved_hash,
        "public hash did not change when a track-caller definition moved",
    );
}
