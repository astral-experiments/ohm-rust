// The compiler flags no-link (and by extension, link-only) used to be broken
// due to changes in encoding/decoding. This was patched, and this test checks
// that these flags are not broken again, resulting in successful compilation.
// See https://github.com/rust-lang/rust/issues/77857

//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn main() {
    rustc().stdin_buf(b"fn main(){}").arg("-Zno-link").arg("-").run();
    rustc().arg("-Zlink-only").input("rust_out.rlink").run();
    run("rust_out");

    rustc()
        .stdin_buf(b"fn main(){}")
        .arg("-Zemit-rlink")
        .arg("-Ztemps-dir=simultaneous-objects")
        .arg("-o")
        .arg("simultaneous")
        .arg("-")
        .run();
    run("simultaneous");

    let mut object_count = 0;
    rfs::read_dir_entries("simultaneous-objects", |path| {
        if path.extension().is_some_and(|extension| extension == "o" || extension == "obj") {
            object_count += 1;
        }
        assert!(!path.extension().is_some_and(|extension| extension == "bc"));
    });
    assert!(object_count > 0);

    for _ in 0..2 {
        rustc().arg("-Zemit-rlink").arg("-Zlink-only").input("simultaneous.rlink").run();
        run("simultaneous");
    }
}
