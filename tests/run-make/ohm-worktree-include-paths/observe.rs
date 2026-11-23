extern crate proc_macro;

use proc_macro::{Literal, Span, TokenStream, TokenTree};

#[proc_macro]
pub fn filename(_: TokenStream) -> TokenStream {
    TokenTree::Literal(Literal::string(&Span::call_site().file())).into()
}
