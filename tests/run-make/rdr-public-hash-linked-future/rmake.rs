//@ ignore-cross-compile
//@ needs-dynamic-linking

use run_make_support::{dynamic_lib_name, rfs, run, rustc};

fn main() {
    let full = rfs::read_to_string("producer.rs");
    let (without_local_user, _) = full.split_once("pub fn producer_user").unwrap();

    // Removing the local user removes the exported drop glue. Consumers
    // then need the MIR again, including in an existing incremental cache.
    for (src, always, linked) in [
        (&*full, false, true),
        (without_local_user, false, false),
        (&*full, false, true),
        (&*full, true, false),
    ] {
        rfs::write("input.rs", src);
        let mut producer = rustc();
        producer
            .input("input.rs")
            .crate_name("producer")
            .crate_type("rlib")
            .edition("2024")
            .arg("-Cincremental=producer-incr")
            .arg("-Zpublic-api-hash")
            .arg("-Zshare-generics=yes");
        if always {
            producer.arg("-Zalways-encode-mir");
        }
        producer.run();
        let output = rustc()
            .input("consumer.rs")
            .edition("2024")
            .extern_("producer", "libproducer.rlib")
            .arg("-Cincremental=consumer-incr")
            .arg("-Zshare-generics=no")
            .arg("-Zprint-mono-items")
            .run()
            .stdout_utf8();
        assert_eq!(output.contains("fn producer::fetch::{closure#0} @@"), !linked);
        run("consumer");
        if linked {
            // Calls in a third crate must have default visibility even with
            // optional sharing switched off. Exercise the MIR inliner too.
            rfs::write("middle.rs", "pub use producer::*;");
            rustc()
                .input("middle.rs")
                .edition("2024")
                .crate_type("dylib")
                .extern_("producer", "libproducer.rlib")
                .arg("-Cprefer-dynamic")
                .run();
            rustc()
                .input("consumer.rs")
                .edition("2024")
                .extern_("producer", dynamic_lib_name("middle"))
                .arg("-Zshare-generics=no")
                .arg("-Zinline-mir")
                .arg("-Zmir-opt-level=4")
                .arg("-Copt-level=2")
                .arg("-Cprefer-dynamic")
                .run();
            run("consumer");
        }
    }
}
