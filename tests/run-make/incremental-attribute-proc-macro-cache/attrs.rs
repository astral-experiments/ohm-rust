#![crate_type = "proc-macro"]
#![feature(
    proc_macro_diagnostic,
    proc_macro_expand,
    proc_macro_tracked_env,
    proc_macro_tracked_path
)]

extern crate proc_macro;

use proc_macro::{Delimiter, Span, TokenStream, TokenTree, tracked};

const VERSION: u32 = 1;

#[proc_macro_attribute]
pub fn cached(annotation: TokenStream, mut item: TokenStream) -> TokenStream {
    // Deliberately untracked output is a test oracle for actual execution, not a dependency.
    eprintln!("EXECUTED cached attribute");
    Span::call_site().warning("cached attribute warning").emit();
    item.extend(
        format!(
            "fn annotation_value() -> u32 {{ {annotation} }}
             fn macro_value() -> u32 {{ {VERSION} }}"
        )
        .parse::<TokenStream>()
        .unwrap(),
    );
    item.extend(
        "macro_rules! cached_identity {
            ($value:expr) => {{ let captured = 0; let _ = captured; $value }};
        }"
        .parse::<TokenStream>()
        .unwrap(),
    );
    item
}

#[proc_macro_attribute]
pub fn tracked_env(_: TokenStream, _: TokenStream) -> TokenStream {
    eprintln!("EXECUTED tracked environment");
    let value = tracked::env_var("ATTR_CACHE_ENV").unwrap();
    format!("fn env_value() -> u32 {{ {value} }}").parse().unwrap()
}

#[proc_macro_attribute]
pub fn tracked_path(_: TokenStream, _: TokenStream) -> TokenStream {
    eprintln!("EXECUTED tracked path");
    tracked::path("tracked.txt");
    let value = std::fs::read_to_string("tracked.txt").unwrap();
    format!("fn path_value() -> u32 {{ {value} }}").parse().unwrap()
}

#[proc_macro_attribute]
pub fn source_text(_: TokenStream, mut item: TokenStream) -> TokenStream {
    eprintln!("EXECUTED source text");
    let body = item
        .clone()
        .into_iter()
        .find_map(|token| match token {
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => Some(group),
            _ => None,
        })
        .unwrap();
    let text = body.span().source_text().unwrap();
    item.extend(
        format!("fn source_value() -> &'static str {{ {text:?} }}").parse::<TokenStream>().unwrap(),
    );
    item
}

#[proc_macro_attribute]
pub fn expanded_env(_: TokenStream, _: TokenStream) -> TokenStream {
    eprintln!("EXECUTED expanded expression");
    let expr: TokenStream = "env!(\"ATTR_CACHE_EXPANDED\")".parse().unwrap();
    let value = expr.expand_expr().unwrap();
    format!("fn expanded_value() -> &'static str {{ {value} }}").parse().unwrap()
}

#[proc_macro_attribute]
pub fn panicking(_: TokenStream, _: TokenStream) -> TokenStream {
    panic!("attribute panic sentinel")
}
