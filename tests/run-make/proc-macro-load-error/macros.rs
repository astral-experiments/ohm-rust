extern crate proc_macro;

use proc_macro::TokenStream;

#[link(name = "missing_helper")]
extern "C" {
    fn answer() -> u32;
}

#[proc_macro]
pub fn make_answer(_: TokenStream) -> TokenStream {
    format!("const ANSWER: u32 = {};", unsafe { answer() }).parse().unwrap()
}
