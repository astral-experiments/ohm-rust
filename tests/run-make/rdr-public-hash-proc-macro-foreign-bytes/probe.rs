extern crate proc_macro;

use proc_macro::{Literal, TokenStream, TokenTree};

#[proc_macro]
pub fn read_span(input: TokenStream) -> TokenStream {
    let span = input.into_iter().next().unwrap().span();
    TokenTree::Literal(Literal::string(&format!("{span:?}"))).into()
}

#[proc_macro]
pub fn read_text(input: TokenStream) -> TokenStream {
    let text = input.into_iter().next().unwrap().span().source_text();
    TokenTree::Literal(Literal::string(&format!("{text:?}"))).into()
}

#[proc_macro]
pub fn discard(input: TokenStream) -> TokenStream {
    drop(input.clone());
    TokenTree::Literal(Literal::u8_suffixed(0)).into()
}
