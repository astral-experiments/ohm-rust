//@ ignore-cross-compile

use run_make_support::{rfs, rustc};

fn check(import: &str, value: u32) -> String {
    let dep = r#"
#![feature(decl_macro)]
pub const A: u32 = 1;
pub const B: u32 = 2;
use self::IMPORT as SELECTED;

const _: () = {
    macro generate($name:ident) {
        #[macro_export]
        macro_rules! $name { () => { SELECTED } }
    }
    generate!(value);
};
"#;
    rfs::write("dep.rs", dep.replace("IMPORT", import));
    rustc().input("dep.rs").crate_type("rlib").arg("-Zpublic-api-hash").emit("metadata").run();
    rfs::write("client.rs", format!("const _: () = assert!(dep::value!() == {value});"));
    rustc()
        .input("client.rs")
        .crate_type("rlib")
        .extern_("dep", "libdep.rmeta")
        .emit("metadata")
        .run();
    rustc().input("libdep.rmeta").arg("-Zls=public_hash").run().stdout_utf8()
}

fn main() {
    // The exported macro inherits definition-site tokens from the private generator.
    assert_ne!(check("A", 1), check("B", 2));
}
