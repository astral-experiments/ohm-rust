extern crate proc_macro;

use proc_macro::{TokenStream, TokenTree};

#[proc_macro]
pub fn doc_literals(input: TokenStream) -> TokenStream {
    let mut rows = Vec::new();
    for token in input {
        if let TokenTree::Group(attribute) = token {
            let Some(TokenTree::Literal(literal)) = attribute.stream().into_iter().nth(2) else {
                panic!("expected a doc string");
            };
            let spelling = literal.to_string();
            rows.push(format!("({literal}, {spelling:?})"));
        }
    }
    format!("&[{}]", rows.join(",")).parse().unwrap()
}
