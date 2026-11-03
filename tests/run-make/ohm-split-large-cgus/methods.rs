extern crate proc_macro;

use proc_macro::{TokenStream, TokenTree};

#[proc_macro_derive(Scored)]
pub fn scored(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter();
    assert!(tokens.any(|token| matches!(token, TokenTree::Ident(t) if t.to_string() == "struct")));
    let Some(TokenTree::Ident(name)) = tokens.next() else { panic!("expected a struct name") };
    format!(
        "#[automatically_derived] impl Score for {name} {{
            #[inline(never)] fn score(&self, x: i64) -> i64 {{
                std::hint::black_box(self.0 * x + Self::OFFSET)
            }}
        }}"
    )
    .parse()
    .unwrap()
}
