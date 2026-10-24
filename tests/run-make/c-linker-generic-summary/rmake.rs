//@ only-linux
//@ ignore-cross-compile
//@ needs-dynamic-linking
//@ needs-crate-type: dylib

// Ordinary shared generics need no C exports, but offload kernels do. Rust
// dylibs must re-export both. Reuse incremental directories across transitions
// in both directions so the producer's changed summary invalidates consumers.

use run_make_support::symbols::exported_dynamic_symbol_names;
use run_make_support::{dynamic_lib_name, object, rust_lib_name, rustc};

fn main() {
    for offload in [false, true, false] {
        let mut producer = rustc();
        producer
            .input("producer.rs")
            .crate_type("rlib")
            .incremental("producer-incremental")
            .arg("-Zshare-generics=yes")
            .arg("-Csymbol-mangling-version=v0");
        if offload {
            producer.cfg("offload");
        }
        producer.run();

        for (name, crate_type) in [("consumer_c", "cdylib"), ("consumer_rust", "dylib")] {
            rustc()
                .input("consumer.rs")
                .crate_name(name)
                .crate_type(crate_type)
                .extern_("producer", rust_lib_name("producer"))
                .incremental(format!("{name}-incremental"))
                .arg("-Zshare-generics=yes")
                .arg("-Csymbol-mangling-version=v0")
                .run();

            let bytes = std::fs::read(dynamic_lib_name(name)).unwrap();
            let file = object::File::parse(bytes.as_slice()).unwrap();
            let exports = exported_dynamic_symbol_names(&file);
            assert!(exports.iter().any(|name| *name == "api"), "{exports:?}");
            let ordinary = exports.iter().any(|name| name.contains("ordinary"));
            let kernel = exports.iter().any(|name| name.contains("offload_candidate"));
            assert_eq!(ordinary, crate_type == "dylib", "{crate_type}: {exports:?}");
            assert_eq!(kernel, offload || crate_type == "dylib", "{crate_type}: {exports:?}");
        }
    }
}
