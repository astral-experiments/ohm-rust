#![feature(proc_macro_diagnostic)]

extern crate proc_macro;

use proc_macro::{Delimiter, Group, TokenStream, TokenTree};

#[proc_macro_derive(Answer)]
pub fn answer(input: TokenStream) -> TokenStream {
    let name = input
        .into_iter()
        .skip_while(
            |token| !matches!(token, TokenTree::Ident(ident) if ident.to_string() == "struct"),
        )
        .nth(1)
        .unwrap();
    format!("impl {name} {{ fn answer() -> u32 {{ 42 }} }}").parse().unwrap()
}

#[proc_macro_attribute]
pub fn identity(_annotation: TokenStream, annotated: TokenStream) -> TokenStream {
    annotated
}

#[proc_macro_attribute]
pub fn body(annotation: TokenStream, annotated: TokenStream) -> TokenStream {
    annotated
        .into_iter()
        .map(|token| match token {
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
                let mut replacement = Group::new(Delimiter::Brace, annotation.clone());
                replacement.set_span(group.span());
                TokenTree::Group(replacement)
            }
            other => other,
        })
        .collect()
}

#[proc_macro_attribute]
pub fn version(_annotation: TokenStream, annotated: TokenStream) -> TokenStream {
    let value = if cfg!(version_two) { "2" } else { "1" };
    body(value.parse().unwrap(), annotated)
}

#[proc_macro_attribute]
pub fn warning(annotation: TokenStream, annotated: TokenStream) -> TokenStream {
    annotation.into_iter().next().unwrap().span().warning("attribute cache warning").emit();
    annotated
}

#[proc_macro_attribute]
pub fn panicking(_annotation: TokenStream, _annotated: TokenStream) -> TokenStream {
    panic!("attribute cache panic");
}

#[proc_macro_attribute]
pub fn invalid(_annotation: TokenStream, _annotated: TokenStream) -> TokenStream {
    "fn".parse().unwrap()
}
