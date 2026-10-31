#![crate_type = "proc-macro"]

extern crate proc_macro;
use proc_macro::TokenStream;

unsafe extern "C" {
    fn value() -> i32;
}

#[proc_macro_derive(Observe)]
pub fn observe(_: TokenStream) -> TokenStream {
    format!("const DERIVED: i32 = {};", unsafe { value() }).parse().unwrap()
}

#[proc_macro_attribute]
pub fn observe_attr(_: TokenStream, _: TokenStream) -> TokenStream {
    format!("const ATTR: i32 = {};", unsafe { value() }).parse().unwrap()
}
