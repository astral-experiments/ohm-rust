//@ ignore-cross-compile

use std::fmt::Write;

use run_make_support::{rfs, rustc};

fn public_hash(source: &str) -> String {
    rfs::write("dep.rs", source);
    rustc().input("dep.rs").crate_type("lib").emit("metadata").arg("-Zpublic-api-hash").run();
    rustc().input("libdep.rmeta").arg("-Zls=public_hash").run().stdout_utf8()
}

fn main() {
    // There are 2^128 paths from Root to A128. Adding the same fingerprint
    // once per path would cancel out any change to A128 in a 128-bit hash.
    let mut source = String::from(
        "pub use inner::Root;\nmod inner {\n\
         pub struct Root { pub a: &'static A0, pub b: &'static B0 }\n",
    );
    for i in 0..128 {
        let next = i + 1;
        for name in ["A", "B"] {
            writeln!(
                source,
                "pub struct {name}{i} {{ pub a: &'static A{next}, pub b: &'static B{next} }}"
            )
            .unwrap();
        }
    }
    source.push_str("pub struct A128 { pub value: u32 }\npub struct B128;\n}\n");

    assert_ne!(public_hash(&source), public_hash(&source.replace("u32", "i32")));
}
