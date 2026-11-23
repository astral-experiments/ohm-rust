extern crate proc_macro;

use proc_macro::TokenStream;

#[proc_macro_derive(Describe, attributes(helper))]
pub fn describe(input: TokenStream) -> TokenStream {
    let example = input.to_string().contains("example");
    format!("impl crate::Marker for Rule {{}} pub const EXAMPLE: bool = {example};")
        .parse()
        .unwrap()
}
