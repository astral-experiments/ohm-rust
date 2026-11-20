extern crate proc_macro;
use proc_macro::{Ident, Span, TokenStream, TokenTree};

#[proc_macro]
pub fn make_mixed(input: TokenStream) -> TokenStream {
    let ty = input.into_iter().next().unwrap();
    let tokens: TokenStream = "<TypeHole>::Node { value: 4 }".parse().unwrap();
    tokens
        .into_iter()
        .map(|t| match t {
            TokenTree::Ident(ref ident) if ident.to_string() == "TypeHole" => ty.clone(),
            TokenTree::Ident(ref ident) if ident.to_string() == "Node" => {
                TokenTree::Ident(Ident::new("Node", Span::mixed_site()))
            }
            other => other,
        })
        .collect()
}
