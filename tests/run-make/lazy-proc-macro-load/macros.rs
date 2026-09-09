extern crate proc_macro;

use std::io::Write;

use proc_macro::TokenStream;

// ELF and Mach-O constructors run when the compiler maps this library, even if
// it never calls a macro. A positive cold check validates the load witness.
#[used]
#[cfg_attr(target_vendor = "apple", unsafe(link_section = "__DATA,__mod_init_func"))]
#[cfg_attr(not(target_vendor = "apple"), unsafe(link_section = ".init_array"))]
static INITIALIZE: extern "C" fn() = loaded;

extern "C" fn loaded() {
    record(b"L");
}

fn record(event: &[u8]) {
    let path = std::env::var_os("LAZY_PROC_MACRO_EVENTS").expect("missing event path");
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("cannot open event file")
        .write_all(event)
        .expect("cannot record macro event");
}

#[proc_macro_derive(Witness, attributes(helper))]
pub fn witness(_: TokenStream) -> TokenStream {
    record(b"D");
    let version = if cfg!(new_version) { 2 } else { 1 };
    format!("impl Input {{ pub const VERSION: u32 = {version}; }}").parse().unwrap()
}

#[proc_macro]
pub fn answer(_: TokenStream) -> TokenStream {
    record(b"B");
    "pub const ANSWER: u32 = 42;".parse().unwrap()
}

#[proc_macro_attribute]
pub fn identity(_: TokenStream, item: TokenStream) -> TokenStream {
    record(b"A");
    item
}
