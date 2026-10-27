//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn library() -> String {
    rustc()
        .input("dep.rs")
        .incremental("incremental")
        .opt_level("1")
        .arg("-Zpublic-api-hash")
        .run();
    rustc().input("libdep.rlib").arg("-Zls=public_hash").run().stdout_utf8()
}

fn caller() {
    rustc()
        .input("main.rs")
        .extern_("dep", "libdep.rlib")
        .arg("-Zpublic-api-hash")
        .arg("-Zemit-rlink")
        .arg("-Ztemps-dir=objects")
        .run();
}

fn producer_hash(source: &str, incremental: &str) -> String {
    rustc().input(source).incremental(incremental).opt_level("1").arg("-Zpublic-api-hash").run();
    rustc().input("libproducer.rlib").arg("-Zls=public_hash").run().stdout_utf8()
}

fn reexport() {
    rustc()
        .input("locations/bridge.rs")
        .extern_("producer", "libproducer.rlib")
        .incremental("bridge-incremental")
        .arg("-Zpublic-api-hash")
        .run();
}

fn consume_reexport() {
    rustc()
        .input("locations/consumer.rs")
        .extern_("bridge", "libbridge.rlib")
        .arg("-Ldependency=.")
        .arg("-Zpublic-api-hash")
        .run();
    run("consumer").assert_stdout_equals("7\n");
}

fn main() {
    let original = rfs::read_to_string("hidden.rs");
    let baseline = library();
    caller();
    run("main").assert_stdout_equals("1 1\n");

    // The caller instantiates Layout's size, but never selects its private method or From<Input>.
    let edited = original.replace("    fn adjusted", "\n    fn adjusted").replace("+ 1", "+ 2");
    rfs::write("hidden.rs", &edited);
    assert_eq!(baseline, library());
    rustc().input("main.rlink").arg("-Zemit-rlink").arg("-Zlink-only").run();
    run("main").assert_stdout_equals("1 2\n");

    // Changing that same private type's layout must invalidate the caller's generic code.
    rfs::write("hidden.rs", edited.replace("struct Layout(u8)", "struct Layout(u64)"));
    assert_ne!(baseline, library());
    caller();
    run("main").assert_stdout_equals("8 2\n");

    // The glob reexport encodes its producer's source-file index for Public's identifier.
    let producer = rfs::read_to_string("locations/producer.rs");
    let before = producer_hash("locations/producer.rs", "source-incremental");
    reexport();
    consume_reexport();
    rfs::write("locations/producer.rs", producer.replace("any()", "all()").replace("= u8", "= i8"));
    assert_ne!(before, producer_hash("locations/producer.rs", "source-incremental"));
    reexport();
    consume_reexport();

    // The macro defining Public moves to a different expansion slot. A new caller must
    // resolve its stable identity inside the already-compiled glob reexport.
    let source = "locations/expansions.rs";
    let initial = rfs::read_to_string(source);
    let before = producer_hash(source, "expansion-incremental");
    reexport();
    let bridge = rfs::read("libbridge.rlib");
    rfs::create_dir("ordinary");
    rustc()
        .input("locations/bridge.rs")
        .extern_("producer", "libproducer.rlib")
        .output("ordinary/libbridge.rlib")
        .run();
    rfs::write(source, initial.replace("cfg(all())", "cfg(any())"));
    assert_eq!(before, producer_hash(source, "expansion-incremental"));
    consume_reexport();
    assert_eq!(bridge, rfs::read("libbridge.rlib"));

    // A bridge compiled without RDR stores raw DefIndices. It must reject the changed producer.
    rustc()
        .input("locations/consumer.rs")
        .extern_("bridge", "ordinary/libbridge.rlib")
        .arg("-Ldependency=.")
        .run_fail()
        .assert_stderr_contains("E0460");

    let removed = initial.replace("public!();", "pub struct Public { pub value: u8 }");
    rfs::write(source, removed);
    assert_ne!(before, producer_hash(source, "expansion-incremental"));
    reexport();
    consume_reexport();
}
