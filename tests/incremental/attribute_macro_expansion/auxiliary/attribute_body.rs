extern crate proc_macro;

use proc_macro::{Delimiter, Group, TokenStream, TokenTree};

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
