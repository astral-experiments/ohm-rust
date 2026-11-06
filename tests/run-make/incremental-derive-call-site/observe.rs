extern crate proc_macro;

use proc_macro::{Span, TokenStream};

#[proc_macro_derive(Observe)]
pub fn observe(_: TokenStream) -> TokenStream {
    let location = Span::call_site().line();
    format!(
        r#"impl Subject {{
            #[rustc_clean(cfg = "after_bytes", loaded_from_disk = "typeck_root")]
            #[rustc_clean(cfg = "after_later", loaded_from_disk = "typeck_root")]
            #[rustc_clean(cfg = "after_translation")]
            fn unchanged(s: &str) -> usize {{ s.len() }}
            fn location() -> usize {{ {location} }}
        }}"#
    )
    .parse()
    .unwrap()
}
