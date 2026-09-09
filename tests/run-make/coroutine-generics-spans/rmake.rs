use run_make_support::{rfs, rust_lib_name, rustc};

fn compile_library() {
    rustc().input("library.rs").crate_type("rlib").edition("2021").debuginfo("2").run();
}

fn compile_client(revision: &str) {
    rustc()
        .input("client.rs")
        .edition("2021")
        .extern_("library", rust_lib_name("library"))
        .incremental("incremental")
        .codegen_units(256)
        .debuginfo("2")
        .arg("-Zquery-dep-graph")
        .arg("-Zincremental-verify-ich")
        .cfg(revision)
        .run();
}

fn main() {
    compile_library();
    compile_client("seed");
    compile_client("reuse");

    // Move the unchanged coroutine and its saved-local spans in the source file.
    let source = rfs::read_to_string("library.rs");
    rfs::write("library.rs", format!("\n{source}"));
    compile_library();
    compile_client("reuse");
}
