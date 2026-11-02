//@ only-unix
//@ needs-target-std

use std::fs::OpenOptions;
use std::io::Write;
use std::thread;

use run_make_support::assertion_helpers::assert_contains;
use run_make_support::{cmd, rfs, rustdoc};

fn main() {
    rfs::write("lib.rs", "pub fn f() {}");
    rfs::create_dir("doc");
    rfs::write("replacement.html", "<p>second</p>\n");
    cmd("mkfifo").arg("fragment.html").run();

    let writer = thread::spawn(|| {
        let mut pipe = OpenOptions::new().write(true).open("fragment.html").unwrap();
        // Once rustdoc opens the pipe, any subsequent open sees different contents.
        rfs::rename("replacement.html", "fragment.html");
        pipe.write_all(b"<p>first</p>\n").unwrap();
    });
    rustdoc()
        .input("lib.rs")
        .arg("--crate-name=probe")
        .arg("--html-in-header=fragment.html")
        .arg("-Zunstable-options")
        .arg("-Zchecksum-hash-algorithm=sha256")
        .emit("dep-info,html-non-static-files,html-static-files")
        .run();
    writer.join().unwrap();

    assert_contains(rfs::read_to_string("doc/probe/index.html"), "<p>first</p>");
    // SHA-256 of the fragment written to the pipe.
    assert_contains(
        rfs::read_to_string("doc/probe.d"),
        "23e2d37d27d30395877d617b640720ac31058bfdc0d7baf1adc28e562b919df0",
    );
}
