extern crate proc_macro;

use proc_macro::{TokenStream, TokenTree};

fn name(input: TokenStream) -> String {
    let mut tokens = input.into_iter();
    while let Some(token) = tokens.next() {
        if matches!(token, TokenTree::Ident(ref value) if value.to_string() == "struct") {
            return tokens.next().unwrap().to_string();
        }
    }
    unreachable!()
}

#[proc_macro_derive(ArchiveLike)]
pub fn archive(input: TokenStream) -> TokenStream {
    let ty = name(input);
    format!(
        "impl Archive for {ty} where u8: Copy {{
             type Archived = Stored{ty};
             fn archive_value() -> u32 {{ 5 }}
         }}
         impl Extra for {ty} {{
             fn extra_value() -> u32 {{ 11 }}
         }}"
    )
    .parse()
    .unwrap()
}

#[proc_macro_derive(DecodeLike, attributes(answer))]
pub fn decode(input: TokenStream) -> TokenStream {
    let spelling = input.to_string();
    let ty = name(input);
    let value = if spelling.contains("nine") { "9" } else { "7" };
    let result = if spelling.contains("wrong") { "bool" } else { "u32" };
    let value = if result == "bool" { "true" } else { value };
    format!(
        "impl<D: ?Sized> Decode<{ty}, D> for <{ty} as Archive>::Archived
         where {ty}: Archive {{
             fn decode_value(_: &D) -> {result} {{ {value} }}
         }}"
    )
    .parse()
    .unwrap()
}
