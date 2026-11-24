//@ ignore-cross-compile

use run_make_support::{rfs, rustc};

/// Read the public hash that Cargo uses to decide whether to rerun downstream rustdoc.
fn public_hash(source: &str) -> String {
    rfs::write("dep.rs", source);
    rustc()
        .input("dep.rs")
        .crate_type("lib")
        .emit("metadata")
        .arg("-Zpublic-api-hash")
        .incremental("cache")
        .run();
    rustc().input("libdep.rmeta").arg("-Zls=public_hash").run().stdout_utf8()
}

fn main() {
    let dep = r#"
#![allow(unused)]
pub struct Alpha;
pub struct Bravo;
pub trait Upper { fn one() {} }
pub trait Lower { fn two() {} }
impl Upper for Alpha {}
impl Lower for Alpha {}

mod names {
    pub use super::Alpha as Chosen;
    pub use super::Alpha as Unused;
    use super::Upper as _;
    /// See [Chosen] and [super::Alpha::one].
    pub fn open() {}
}
pub use names::open;
"#;
    let original = public_hash(dep);
    assert_ne!(original, public_hash(&dep.replace("Alpha as Chosen", "Bravo as Chosen")));
    assert_ne!(original, public_hash(&dep.replace("Upper as _", "Lower as _")));
    assert_eq!(original, public_hash(&dep.replace("Alpha as Unused", "Bravo as Unused")));
}
