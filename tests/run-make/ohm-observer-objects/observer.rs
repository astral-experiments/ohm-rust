extern crate proc_macro;

#[proc_macro]
pub fn produce(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let span = input.into_iter().next().unwrap().span();
    assert_eq!(span.source_text().as_deref(), Some("foreign_token"));
    let number = std::fs::read_to_string("number").unwrap();
    format!(
        "#[unsafe(no_mangle)] #[inline(never)] \
         pub extern \"C\" fn ohm_observed_value() -> u32 {{ {number} }}"
    )
    .parse()
    .unwrap()
}
