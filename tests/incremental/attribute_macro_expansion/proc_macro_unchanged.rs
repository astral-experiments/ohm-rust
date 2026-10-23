//@ proc-macro:attribute_body.rs
//@ revisions:rpass1 rpass2
//@ compile-flags: -Zquery-dep-graph -Zcache-proc-macros
//@ ignore-backends: gcc

#![feature(rustc_attrs)]

extern crate attribute_body;

mod left {
    const VALUE: u32 = 10;

    #[rustc_clean(cfg = "rpass2", loaded_from_disk = "attribute_macro_expansion")]
    #[attribute_body::body(VALUE)]
    pub fn value() -> u32 {
        0
    }
}

mod right {
    const VALUE: u32 = 20;

    #[rustc_clean(cfg = "rpass2", loaded_from_disk = "attribute_macro_expansion")]
    #[attribute_body::body(VALUE)]
    pub fn value() -> u32 {
        0
    }
}

#[rustc_clean(cfg = "rpass2", loaded_from_disk = "attribute_macro_expansion")]
#[attribute_body::body(1)]
#[attribute_body::body(2)]
pub fn ordered() -> u32 {
    0
}

fn main() {
    assert_eq!(left::value(), 10);
    assert_eq!(right::value(), 20);
    assert_eq!(ordered(), 2);
}
