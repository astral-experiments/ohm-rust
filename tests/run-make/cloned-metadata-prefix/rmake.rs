//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn main() {
    let source = rfs::read_to_string("library.rs");
    let mut retained = Vec::new();
    for (revision, enabled) in ["0", "1", "1", "0", "1"].into_iter().enumerate() {
        let value = 10 + revision;
        rfs::write("library.rs", source.replace("    10", &format!("    {value}")));
        rustc()
            .input("library.rs")
            .incremental("incremental")
            .arg("--emit=link,metadata")
            .arg("-Zpublic-api-hash")
            .arg("-Zincremental-verify-ich")
            .env("RUSTC_EXPERIMENTAL_CLONED_METADATA_PREFIX", enabled)
            .run();
        rustc().input("main.rs").extern_("library", "liblibrary.rlib").run();
        run("main").assert_stdout_equals(format!("{value}\n"));

        for (path, bytes) in &retained {
            assert_eq!(&rfs::read(path), bytes, "retained metadata was modified");
        }
        let path = format!("retained-{revision}.rmeta");
        std::fs::hard_link("liblibrary.rmeta", &path).unwrap();
        retained.push((path, rfs::read("liblibrary.rmeta")));
    }
}
