//@ needs-target-std
use run_make_support::{rfs, rustc};

fn main() {
    #[cfg(unix)]
    {
        checksum_uses_the_literal_filename();
        lossy_extern_paths();
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

#[cfg(unix)]
fn lossy_extern_paths() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    // These two real directories produce the same UTF-8 spelling in the depfile.
    let invalid = Path::new(OsStr::from_bytes(b"input-\xff"));
    let valid = Path::new("input-\u{fffd}");
    for (name, dir) in [("left", invalid), ("right", valid)] {
        rfs::create_dir(dir);
        rfs::write("input.rs", "pub const VALUE: u8 = 1;");
        let lib = run_make_support::rust_lib_name(name);
        rustc().input("input.rs").crate_name(name).crate_type("rlib").output(&lib).run();
        let target = dir.join("libsame.rlib");
        rfs::copy(&lib, &target);
        rfs::remove_file(&lib);
        symlink(target, lib).unwrap();
    }
    let rendered = std::path::absolute(valid).unwrap().join("libsame.rlib");
    for (crates, checksum) in
        [(&["left", "right"][..], false), (&["right", "left"], false), (&["right"], true)]
    {
        let mut command = rustc();
        let mut source = String::new();
        for name in crates {
            source += &format!("extern crate {name}; const _: u8 = {name}::VALUE;\n");
            command.extern_(name, run_make_support::rust_lib_name(name));
        }
        rfs::write("read.rs", source);
        command
            .input("read.rs")
            .crate_type("rlib")
            .arg("-Zbinary-dep-depinfo")
            .arg("-Zchecksum-hash-algorithm=sha256")
            .emit("dep-info=read.d,metadata")
            .run();
        let dep = rfs::read_to_string("read.d");
        let rendered = rendered.display().to_string().replace(' ', "\\ ");
        assert!(dep.lines().any(|line| line == format!("{rendered}:")));
        assert_eq!(
            dep.lines().any(|line| line.starts_with("# checksum:") && line.ends_with(&rendered)),
            checksum
        );
    }
}
