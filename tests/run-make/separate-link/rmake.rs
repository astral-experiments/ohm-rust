// The compiler flags no-link (and by extension, link-only) used to be broken
// due to changes in encoding/decoding. This was patched, and this test checks
// that these flags are not broken again, resulting in successful compilation.
// See https://github.com/rust-lang/rust/issues/77857

//@ ignore-cross-compile

use run_make_support::{build_native_static_lib, rfs, run, rustc};

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

    rfs::write("answer.c", "int answer(void) { return 1; }");
    let native_library = build_native_static_lib("answer");
    rustc()
        .stdin_buf(
            br#"
            #[link(name = "answer", kind = "static", modifiers = "-bundle")]
            unsafe extern "C" { fn answer() -> i32; }
            fn main() { println!("{}", unsafe { answer() }); }
        "#,
        )
        .crate_name("native")
        .arg("-Lnative=.")
        .arg("-Zemit-rlink")
        .arg("-")
        .run();
    run("native").assert_stdout_equals("1\n");

    rfs::write("answer.c", "int answer(void) { return 2; }");
    build_native_static_lib("answer");
    rustc().arg("-Lnative=.").arg("-Zlink-only").input("native.rlink").run();
    run("native").assert_stdout_equals("2\n");
    rfs::remove_file(native_library);
    rustc().arg("-Lnative=.").arg("-Zlink-only").input("native.rlink").run_fail();
}
