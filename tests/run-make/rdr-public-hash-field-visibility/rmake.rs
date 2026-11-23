//@ ignore-cross-compile

use run_make_support::{rfs, rustc};

fn public_hash(path: &str) -> String {
    rustc().input(path).arg("-Zls=public_hash").run().stdout_utf8()
}

fn hashes(dep: &str, facade: &str) -> (String, String) {
    rfs::write("dep.rs", dep);
    rustc()
        .input("dep.rs")
        .crate_type("lib")
        .emit("metadata")
        .arg("-Zpublic-api-hash")
        .incremental("dep-cache")
        .run();
    rfs::write("facade.rs", facade);
    rustc()
        .input("facade.rs")
        .edition("2021")
        .crate_type("lib")
        .emit("metadata")
        .extern_("dep", "libdep.rmeta")
        .arg("-Zpublic-api-hash")
        .incremental("facade-cache")
        .run();
    (public_hash("libdep.rmeta"), public_hash("libfacade.rmeta"))
}

fn main() {
    let source = "pub struct Value { pub(crate) field: u32 }\npub const OTHER: u32 = 11;\n";
    let changed = source.replace("11", "12");
    let facade = "pub fn take(_: dep::Value) {}";
    let (direct, indirect) = hashes(source, facade);
    let (new_direct, new_indirect) = hashes(&changed, facade);
    assert_ne!(direct, new_direct);
    assert_eq!(indirect, new_indirect);

    // The field still contributes its visibility and type.
    assert_ne!(indirect, hashes(&source.replace("pub(crate)", "pub       "), facade).1);
    assert_ne!(indirect, hashes(&source.replace("field: u32", "field: u64"), facade).1);

    // A reexport of the module itself must still expose changes to OTHER.
    let facade = "pub use dep as api;";
    assert_ne!(hashes(source, facade).1, hashes(&changed, facade).1);

    // A consumer may see an impl even when the facade doesn't name its type.
    let dep = format!("{source}pub struct Extra;");
    let with_impl =
        format!("{dep} impl From<Extra> for u32 {{ fn from(_: Extra) -> u32 {{ 0 }} }}");
    let facade = "pub use dep::Value;";
    assert_ne!(hashes(&dep, facade).1, hashes(&with_impl, facade).1);

    // A reexported macro can name a different item via its original crate.
    let dep = "#[macro_export] macro_rules! fetch { () => { $crate::helper() } }
               pub fn helper() -> u32 { 0 }";
    let facade = "pub use dep::fetch;";
    assert_ne!(hashes(dep, facade).1, hashes(&dep.replace("u32", "i32"), facade).1);
}
