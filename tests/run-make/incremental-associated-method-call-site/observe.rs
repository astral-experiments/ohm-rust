extern crate proc_macro;

use proc_macro::{Span, TokenStream};

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
