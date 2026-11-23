//@ revisions: cpass1 cpass2 cpass3
//@ compile-flags: -Zquery-dep-graph
#![feature(rustc_attrs)]
#![allow(internal_features)]
#![crate_type = "rlib"]

pub struct Parser;

impl Parser {
    fn parse(&self, input: &str) -> usize {
        #[cfg(cpass2)]
        let _trim = |s: &str| s.trim().len();
        input.len()
    }
}

// Inserting and removing the closure must leave both lifetimes in
// Parser::parse's signature unchanged.
#[rustc_clean(cfg = "cpass2", loaded_from_disk = "typeck_root")]
#[rustc_clean(cfg = "cpass3", loaded_from_disk = "typeck_root")]
pub fn caller(input: &str) -> usize {
    Parser.parse(input)
}
