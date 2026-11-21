//@ needs-target-std

use run_make_support::{Rustc, rfs, rustc};

fn producer(public_hash: bool, ty: &str) {
    rfs::write("producer.rs", format!("pub fn value() -> {ty} {{ 0 }}"));
    let mut cmd = rustc();
    cmd.input("producer.rs").crate_type("rlib").emit("metadata");
    if public_hash {
        cmd.arg("-Zpublic-api-hash");
    }
    cmd.run();
}

fn consumer(state: &str) -> Rustc {
    let mut cmd = rustc();
    cmd.input("consumer.rs")
        .crate_type("rlib")
        .emit("metadata")
        .extern_("producer", "libproducer.rmeta")
        .incremental("incr")
        .arg("-Zpublic-api-hash")
        .arg("-Zohm-shared-stages")
        .arg("-Zincremental-verify-ich")
        .arg(format!("-Zassert-incr-state={state}"));
    cmd
}

fn main() {
    rfs::write("consumer.rs", "pub fn read() -> u32 { producer::value() }");

    // The consumer can depend on the producer's whole-crate hash directly.
    producer(false, "u32");
    consumer("not-loaded").run();
    producer(false, "u64");
    consumer("loaded").run_fail().assert_stderr_contains("expected `u32`, found `u64`");

    // Public API metadata gives the signature its own node. When the producer
    // switches back, that node must still receive a color during validation.
    producer(true, "u32");
    consumer("loaded").run();
    producer(false, "u32");
    consumer("loaded").run();
    producer(true, "u64");
    consumer("loaded").run_fail().assert_stderr_contains("expected `u32`, found `u64`");
}
