extern crate proc_macro;

use proc_macro::TokenStream;

#[proc_macro]
pub fn observe(input: TokenStream) -> TokenStream {
    let token = input.into_iter().next().expect("macro input did not contain a token");
    format!("{}u32", token.span().column()).parse().expect("macro output was not a token stream")
}
