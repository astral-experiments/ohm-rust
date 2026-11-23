//@ ignore-cross-compile
//@ needs-dynamic-linking

use run_make_support::{dynamic_lib_name, rfs, rustc};

fn check(items: &str, cache: &str) -> Vec<u8> {
    rfs::write("items.rs", items);
    rustc()
        .input("dep.rs")
        .crate_type("rlib")
        .edition("2021")
        .extern_("derive", dynamic_lib_name("derive"))
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich")
        .incremental(cache)
        .emit("metadata")
        .run();
    rustc().input("libdep.rmeta").arg("-Zls=public_hash").run().stdout()
}

fn main() {
    rustc().input("derive.rs").edition("2021").crate_type("proc-macro").run();
    // An exported macro keeps the private impl in the public metadata graph.
    rfs::write(
        "dep.rs",
        "#![allow(dead_code)]\n\
         #[macro_export] macro_rules! exported { () => {} }\n\
         pub trait Marker {}\n\
         pub mod items;\n",
    );
    let items = "#[derive(Debug)] struct Earlier;\n\
                 impl Earlier { fn method(&self) {} }\n\
                 /// An example.\n\
                 #[derive(derive::Describe)]\n\
                 #[helper(first)]\n\
                 struct Rule;\n";
    let before = check(items, "warm");
    // The byte distance from the preceding method changes, but all exported
    // coordinates and the expansion of the derive stay the same.
    let wider = items.replace("An example", "An  example");
    assert_eq!(before, check(&wider, "warm"));
    assert_eq!(before, check(&wider, "cold"));
    assert_ne!(before, check(&wider.replace("#[helper", "  #[helper"), "warm"));
    assert_ne!(before, check(&wider.replace("first", "later"), "warm"));
    // A doc edit of the same width can change the exported constant.
    assert_ne!(before, check(&items.replace("example", "account"), "warm"));
    assert_eq!(before, check(items, "warm"));
}
