//@ only-x86_64-unknown-linux-gnu
//@ ignore-cross-compile

use run_make_support::{cargo, cmd, has_extension, path, recursive_find_files, rfs, rustc_path};

fn main() {
    let lib = "probe/src/lib.rs";
    let original = rfs::read_to_string(lib);
    let mut previous = None;

    for (extra, line) in [(0, "3"), (2, "5"), (4, "7")] {
        let call = "    opaque::location()";
        rfs::write(lib, original.replace(call, &format!("{}{call}", "\n".repeat(extra))));
        cargo()
            .args(["build", "--offline", "--bin", "app"])
            .env("RUSTC", rustc_path())
            .env("CARGO_ENCODED_RUSTFLAGS", "-Zohm-check-relink-inputs")
            .env("CARGO_TARGET_DIR", path("target"))
            .env_remove("CARGO_BUILD_BUILD_DIR")
            .run();
        cmd(path("target/debug/app")).run().assert_stdout_equals(line);

        let [record]: [_; 1] = recursive_find_files(path("target/debug/incremental"), |p| {
            has_extension(p, "dwarf-typed")
        })
        .try_into()
        .expect("one caller-location module");
        let packet = rfs::read(&record);
        let object = rfs::read(record.with_extension("o"));
        assert!(object.starts_with(b"\x7fELF"));
        assert!(packet.ends_with(&object), "next edit must start from the current native object");
        if let Some(prior) = previous {
            assert_ne!(prior, object, "the observable caller line moved");
        }
        previous = Some(object);
    }
}
