extern crate proc_macro;

use proc_macro::{Delimiter, Group, Span, TokenStream, TokenTree};

/// Move the attributed method into a wrapper stamped with the impl attribute's location.
#[proc_macro_attribute]
pub fn wrap(_: TokenStream, item: TokenStream) -> TokenStream {
    let mut tokens = item.into_iter().collect::<Vec<_>>();
    let Some(TokenTree::Group(body)) = tokens.pop() else { unreachable!() };
    let mut members = body.stream().into_iter().collect::<Vec<_>>();
    let attribute = members
        .iter()
        .position(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == '#'))
        .unwrap();
    let mut nested: TokenStream = members.drain(attribute..).collect();
    nested.extend("observed()".parse::<TokenStream>().unwrap());
    members.extend("fn observed() -> (usize, usize, usize)".parse::<TokenStream>().unwrap());
    members.push(TokenTree::Group(Group::new(Delimiter::Brace, nested)));
    tokens.push(TokenTree::Group(Group::new(Delimiter::Brace, members.into_iter().collect())));
    tokens.into_iter().collect()
}

#[proc_macro_attribute]
pub fn site(_: TokenStream, mut method: TokenStream) -> TokenStream {
    let span = Span::call_site();
    let (line, column) = (span.line(), span.column());
    let generated = format!(
        "fn observed() -> (usize, usize, usize) {{
            make_inner!();
            ({line}, {column}, inner(\"abc\"))
        }}"
    );
    method.extend(generated.parse::<TokenStream>().unwrap());
    method
}
