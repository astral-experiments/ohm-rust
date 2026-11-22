extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn observe(_: TokenStream, module: TokenStream) -> TokenStream {
    // Inner attribute macros receive the whole loaded module before configuration.
    let text = module.to_string().replace(char::is_whitespace, "");
    assert_eq!(text.matches("[0x01,2]").count(), 255);
    assert!(text.contains("[3,4]"));
    module
}
