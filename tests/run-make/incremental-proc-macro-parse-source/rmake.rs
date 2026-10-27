//@ ignore-cross-compile
//@ needs-crate-type: proc-macro
//@ ignore-musl (FIXME: can't find `-lunwind`)

use std::path::Path;

use run_make_support::{Rustc, dynamic_lib_name, rfs, run, rustc};

fn producer(cache: &str, mode: &str) -> Rustc {
    let mut cmd = rustc();
    cmd.input("producer.rs")
        .edition("2024")
        .cfg(mode)
        .extern_("maker", dynamic_lib_name("maker"))
        .incremental(cache)
        .arg("-Zcache-proc-macros")
        .arg("-Zpublic-api-hash")
        .emit("link,metadata");
    cmd
}

fn save_bridge(history: &str) {
    rustc().input("bridge.rs").extern_("producer", "libproducer.rlib").run();
    rfs::create_dir(history);
    for name in ["libproducer.rlib", "libproducer.rmeta", "libbridge.rlib"] {
        rfs::copy(name, Path::new(history).join(name));
    }
}

fn caller(producer: &str, bridge: &str) -> String {
    for name in ["libproducer.rlib", "libproducer.rmeta"] {
        rfs::copy(Path::new(producer).join(name), name);
    }
    rfs::copy(Path::new(bridge).join("libbridge.rlib"), "libbridge.rlib");
    rustc().input("caller.rs").extern_("bridge", "libbridge.rlib").run();
    run("caller").stdout_utf8()
}

fn compare(mode: &str, original: &str) {
    let cache = format!("{mode}-incremental");
    let warm = format!("{mode}-warm");
    let clean = format!("{mode}-clean");
    let executed = if mode == "cached" { "executed derive" } else { "executed tracked derive" };
    rfs::write("producer.rs", original);
    rfs::write("parse-input.txt", "91");
    producer(&cache, mode).run().assert_stderr_contains(executed);
    rfs::write("producer.rs", original.replace("    7", "    8"));
    rfs::write("parse-input.txt", "379");

    let result = producer(&cache, mode).run();
    if mode == "cached" {
        result
            .assert_stderr_not_contains("executed attribute")
            .assert_stderr_not_contains(executed);
    } else {
        result.assert_stderr_contains(executed);
    }
    save_bridge(&warm);
    producer(&format!("{mode}-empty"), mode).run().assert_stderr_contains(executed);
    save_bridge(&clean);

    // A later invocation embeds both the included file's absolute span and its own
    // call-site and mixed-site syntax contexts in the native program.
    let observed = caller(&clean, &clean);
    assert_eq!(caller(&warm, &warm), observed);
    // Each ordinary bridge must also accept the independently rebuilt producer.
    assert_eq!(caller(&warm, &clean), observed);
    assert_eq!(caller(&clean, &warm), observed);
    assert!(
        rfs::read(Path::new(&warm).join("libproducer.rmeta"))
            == rfs::read(Path::new(&clean).join("libproducer.rmeta"))
    );
}

fn main() {
    rustc().input("maker.rs").run();
    let original = rfs::read_to_string("producer.rs");
    compare("cached", &original);
    compare("tracked", &original);
}
