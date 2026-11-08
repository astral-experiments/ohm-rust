#![crate_type = "proc-macro"]

extern crate proc_macro;
use proc_macro::{Delimiter, Group, TokenStream, TokenTree};

#[proc_macro_attribute]
pub fn add_await(location: TokenStream, item: TokenStream) -> TokenStream {
    let mut tokens: Vec<_> = item.into_iter().collect();
    let TokenTree::Group(body) = tokens.pop().unwrap() else { unreachable!() };

    // Parsed tokens use the whole attribute invocation as their call site.
    let mut expr: TokenStream = "core::future::ready".parse().unwrap();
    expr.extend([TokenTree::Group(Group::new(Delimiter::Parenthesis, body.stream()))]);
    expr.extend(".await".parse::<TokenStream>().unwrap());
    if !location.is_empty() {
        expr.extend(", line!(), column!()".parse::<TokenStream>().unwrap());
        expr = TokenTree::Group(Group::new(Delimiter::Parenthesis, expr)).into();
    }
    let mut replacement = Group::new(Delimiter::Brace, expr);
    replacement.set_span(body.span());
    tokens.push(TokenTree::Group(replacement));
    tokens.into_iter().collect()
}
