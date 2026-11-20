//@ ignore-cross-compile

use run_make_support::{run, rustc};

fn main() {
    let facade = r#"
extern crate alloc;
pub mod exports { pub use alloc as collections; }

mod inner {
    pub trait Ext { fn len(&self) -> usize { 13 } }
    impl Ext for Box<str> {}
}
mod m { pub(crate) use super::inner::Ext; }
use m::*;
pub use inner::*;
"#;
    // Cargo caps lints for registry dependencies, including pub_use_of_private_extern_crate.
    rustc()
        .input("-")
        .stdin_buf(facade)
        .crate_name("facade")
        .crate_type("rlib")
        .edition("2021")
        .arg("--cap-lints=allow")
        .arg("-Zpublic-api-hash")
        .run();

    // Both exports can be reached through an intermediate private import.
    // If Ext is missing, autoderef finds str::len and returns 2.
    let client = r#"
use facade::*;
use exports::collections::boxed::Box;

fn main() {
    let text: Box<str> = "ok".into();
    assert_eq!(text.len(), 13);
}
"#;
    rustc()
        .input("-")
        .stdin_buf(client)
        .crate_name("client")
        .extern_("facade", "libfacade.rlib")
        .edition("2021")
        .run();
    run("client");
}
