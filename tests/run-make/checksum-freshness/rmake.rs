//@ needs-target-std
use run_make_support::{rfs, rustc};

fn main() {
    if cfg!(unix) {
        checksum_uses_the_literal_filename();
    }

    rustc().input("lib.rs").arg("-Zchecksum-hash-algorithm=blake3").emit("dep-info").run();
    let make_file_contents = rfs::read_to_string("lib.d");
    let expected_contents = rfs::read_to_string("expected.d");
    assert_eq!(make_file_contents, expected_contents);
    assert!(!expected_contents.is_empty());
}

fn checksum_uses_the_literal_filename() {
    use std::path::Path;

    use run_make_support::rust_lib_name;

    let name = rust_lib_name("helper");
    rfs::write("helper.rs", "pub fn value() -> u32 { 17 }");
    rfs::write("user.rs", "pub fn value() -> u32 { helper::value() }");
    rustc().input("helper.rs").crate_type("rlib").run();
    rfs::create_dir("actual dir");
    rfs::create_dir(r"actual\ dir");
    let actual = Path::new("actual dir").join(&name);
    rfs::copy(&name, &actual);
    rfs::write(Path::new(r"actual\ dir").join(&name), b"different file behind the Make escape");

    let checksum = |path: &Path, binary_input: bool| {
        let path = std::path::absolute(path).unwrap();
        let mut command = rustc();
        if binary_input {
            command.input("bytes.rs").env("BLOB", &path);
        } else {
            command.input("user.rs").extern_("helper", &path).arg("-Zbinary-dep-depinfo");
        }
        command
            .edition("2021")
            .crate_type("rlib")
            .arg("-Zchecksum-hash-algorithm=sha256")
            .emit("dep-info=user.d,metadata")
            .run();
        let depfile = rfs::read_to_string("user.d");
        let spelling = path.display().to_string().replace(' ', "\\ ");
        depfile
            .lines()
            .find_map(|line| {
                line.strip_prefix("# checksum:")?
                    .strip_suffix(&format!(" {spelling}"))
                    .map(str::to_owned)
            })
            .unwrap()
    };
    assert_eq!(checksum(Path::new(&name), false), checksum(&actual, false));

    rfs::write("bytes.rs", r#"pub const BYTES: &[u8] = include_bytes!(env!("BLOB"));"#);
    rfs::write("blob", [0xff, 17]);
    let actual = Path::new("actual dir/blob");
    rfs::copy("blob", actual);
    rfs::write(r"actual\ dir/blob", [0xff, 29]);
    assert_eq!(checksum(Path::new("blob"), true), checksum(actual, true));
}
