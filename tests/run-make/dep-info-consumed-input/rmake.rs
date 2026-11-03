//@ needs-target-std

use run_make_support::{dynamic_lib_name, rfs, rustc};

fn compile(source: &str) -> String {
    rfs::write("input.rs", source);
    rustc()
        .input("input.rs")
        .crate_type("rlib")
        .edition("2021")
        .extern_("save", dynamic_lib_name("save"))
        .arg("-Zchecksum-hash-algorithm=sha256")
        .emit("metadata,dep-info=input.d")
        .run();
    let deps = rfs::read_to_string("input.d");
    assert!(deps.lines().any(|line| line.ends_with("payload.bin:")));
    deps
}

fn main() {
    rustc().input("save.rs").crate_type("proc-macro").run();
    let first = r#"
        pub const FIRST: &[u8] = include_bytes!("payload.bin");
        const _: [(); 17] = [(); FIRST[0] as usize];
        const _: usize = save::save!(binary);
    "#;
    rfs::write("payload.bin", [17, 255, 134, 93]);
    let deps = compile(first);
    assert_eq!(rfs::read("payload.bin"), [29, 255, 134, 93]);
    assert!(deps.contains("# checksum:sha256=f5d6aa0ffac7194c844ac4a5578cd712ef0fb6dfa0efbef7bdfb182fc648947d file_len:4 "));

    rfs::write("payload.bin", [17, 255, 134, 93]);
    let deps = compile(&format!("{first}\n{}", r#"
        pub const SECOND: &[u8] = include_bytes!("payload.bin");
        const _: [(); 29] = [(); SECOND[0] as usize];
    "#));
    assert!(!deps.lines().any(|line| line.starts_with("# checksum:") && line.ends_with("payload.bin")));

    // PathBuf equality drops inner dots. Cargo must not collapse distinct
    // observations into a seemingly valid last checksum.
    rfs::create_dir("nested");
    rfs::write("nested/payload.bin", [17, 255, 134, 93]);
    let deps = compile(r#"
        pub const FIRST: &[u8] = include_bytes!("nested/payload.bin");
        const _: [(); 17] = [(); FIRST[0] as usize];
        const _: usize = save::save!(nested);
        pub const SECOND: &[u8] = include_bytes!("nested/./payload.bin");
        const _: [(); 29] = [(); SECOND[0] as usize];
    "#);
    assert!(!deps.lines().any(|line| line.starts_with("# checksum:") && line.ends_with("payload.bin")));

    // A valid, empty Rust source and the empty span placeholder for a non-UTF-8
    // binary look alike. Both read orders must leave their shared path unverified.
    rfs::write("payload.bin", []);
    let deps = compile(r#"
        include!("payload.bin");
        const _: usize = save::save!(binary);
        pub const SECOND: &[u8] = include_bytes!("payload.bin");
        const _: [(); 29] = [(); SECOND[0] as usize];
    "#);
    assert!(!deps.lines().any(|line| line.starts_with("# checksum:") && line.ends_with("payload.bin")));

    rfs::write("payload.bin", [17, 255, 134, 93]);
    let deps = compile(r#"
        pub const FIRST: &[u8] = include_bytes!("payload.bin");
        const _: [(); 17] = [(); FIRST[0] as usize];
        save::save!(empty);
    "#);
    assert!(!deps.lines().any(|line| line.starts_with("# checksum:") && line.ends_with("payload.bin")));
}
