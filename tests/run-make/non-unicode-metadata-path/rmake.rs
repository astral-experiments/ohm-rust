//@ only-unix

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

use run_make_support::{rfs, rustc};

fn main() {
    let dir = std::path::Path::new(OsStr::from_bytes(b"crate-\xff"));
    rfs::create_dir(dir);
    rfs::write(dir.join("lib.rs"), "pub fn value() -> u8 { 7 }");
    rustc()
        .current_dir(dir)
        .crate_name("raw_path")
        .crate_type("rlib")
        .input("lib.rs")
        .output("../libraw_path.rlib")
        .run();

    // Loading the dependency exercises path decoding as well as the metadata writer.
    rfs::write("consumer.rs", "extern crate raw_path; pub fn result() -> u8 { raw_path::value() }");
    rustc().crate_type("rlib").input("consumer.rs").extern_("raw_path", "libraw_path.rlib").run();
}
