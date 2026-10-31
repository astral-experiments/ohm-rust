#![feature(proc_macro_span)]

extern crate proc_macro;
use proc_macro::{Literal, TokenStream, TokenTree};

#[proc_macro]
pub fn value(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter();
    let Some(TokenTree::Literal(source)) = tokens.next() else { panic!("expected a string") };
    let offset: usize = tokens.next().unwrap().to_string().parse().unwrap();
    let mut value = Literal::u32_unsuffixed(17);
    value.set_span(source.subspan(offset..offset + 1).unwrap());
    TokenTree::Literal(value).into()
}
