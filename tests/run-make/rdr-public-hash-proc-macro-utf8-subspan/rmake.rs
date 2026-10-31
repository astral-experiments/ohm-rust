//@ ignore-cross-compile

use run_make_support::{dynamic_lib_name, run, rust_lib_name, rustc};

fn main() {
    rustc().input("subspan.rs").crate_type("proc-macro").run();
    rustc()
        .input("span_lib.rs")
        .edition("2021")
        .crate_type("rlib")
        .extern_("subspan", dynamic_lib_name("subspan"))
        .arg("-Zpublic-api-hash")
        .run();
    rustc().input("main.rs").edition("2021").extern_("span_lib", rust_lib_name("span_lib")).run();
    run("main");
}
