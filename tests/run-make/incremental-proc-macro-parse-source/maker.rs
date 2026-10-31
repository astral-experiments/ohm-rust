#![crate_type = "proc-macro"]
#![feature(proc_macro_tracked_path)]

extern crate proc_macro;
use proc_macro::{Literal, Punct, Spacing, TokenStream, TokenTree};

#[proc_macro_attribute]
pub fn parsed(_: TokenStream, item: TokenStream) -> TokenStream {
    eprintln!("executed attribute");
    let _ = "0".parse::<TokenStream>().unwrap();
    let _ = "1234u16".parse::<Literal>().unwrap();
    item
}

#[proc_macro_derive(Parsed)]
pub fn parsed_derive(_: TokenStream) -> TokenStream {
    eprintln!("executed derive");
    // The attribute already allocated this source file.
    let _ = "0".parse::<TokenStream>().unwrap();
    assert!("not_a_literal".parse::<Literal>().is_err());
    "pub const DERIVED_LINE: u32 = line!();".parse().unwrap()
}

#[proc_macro_derive(Tracked)]
pub fn tracked(_: TokenStream) -> TokenStream {
    eprintln!("executed tracked derive");
    let path = "parse-input.txt";
    let value: TokenStream = std::fs::read_to_string(path).unwrap().parse().unwrap();
    // This rejects the cached expansion after its first parse. Replaying that old parse would
    // leave a virtual file for the old input in the source map.
    proc_macro::tracked::path(path);
    let mut output: TokenStream = "pub const TRACKED: u32 =".parse().unwrap();
    output.extend(value);
    output.extend([TokenTree::Punct(Punct::new(';', Spacing::Alone))]);
    output
}

#[proc_macro]
pub fn position(token: TokenStream) -> TokenStream {
    let span = token.into_iter().next().unwrap().span();
    let call = proc_macro::Span::call_site();
    let mixed = proc_macro::Span::mixed_site();
    TokenTree::Literal(Literal::string(&format!("{span:?} {call:?} {mixed:?}"))).into()
}
