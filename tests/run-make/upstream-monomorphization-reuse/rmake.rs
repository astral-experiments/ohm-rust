//@ ignore-cross-compile
// Reason: the executable is run after changing its upstream provider.

use run_make_support::{rfs, run, rust_lib_name, rustc};

fn compile_provider() {
    rustc()
        .input("provider.rs")
        .edition("2021")
        .arg("-Zshare-generics=yes")
        .arg("-Zpublic-api-hash")
        .arg("-Csymbol-mangling-version=v0")
        .extern_("mono_defs", rust_lib_name("mono_defs"))
        .run();
}

fn compile_client(revision: &str) {
    rustc()
        .input("client.rs")
        .edition("2021")
        .cfg(revision)
        .incremental("incremental")
        .codegen_units(256)
        .arg("-Cdebuginfo=line-tables-only")
        .arg("-Csymbol-mangling-version=v0")
        .arg("-Zshare-generics=yes")
        .arg("-Zpublic-api-hash")
        .arg("-Zquery-dep-graph")
        .arg("-Zincremental-verify-ich")
        .extern_("mono_defs", rust_lib_name("mono_defs"))
        .extern_("mono_provider", rust_lib_name("mono_provider"))
        .run();
    run("mono_client");
}

fn main() {
    rustc()
        .input("definitions.rs")
        .edition("2021")
        .arg("-Zshare-generics=yes")
        .arg("-Zpublic-api-hash")
        .arg("-Csymbol-mangling-version=v0")
        .run();
    compile_provider();
    compile_client("seed");
    compile_client("reuse");

    // A second instantiation must not invalidate the consumer of the first.
    let original = rfs::read_to_string("provider.rs");
    let unrelated =
        format!("{original}\npub fn extra() -> u16 {{ mono_defs::identity(19_u16) }}\n");
    rfs::write("provider.rs", &unrelated);
    compile_provider();
    compile_client("reuse");

    // Removing the first provider requires a new local instantiation and symbol.
    rfs::write("provider.rs", unrelated.replace("mono_defs::identity(17_u32)", "17"));
    compile_provider();
    compile_client("provider_removed");
    compile_client("reuse");
}
