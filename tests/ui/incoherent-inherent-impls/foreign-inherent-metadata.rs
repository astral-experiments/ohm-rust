//@ run-pass
//@ aux-build:extern-crate.rs
//@ aux-build:foreign-inherent-extension.rs

extern crate extern_crate;
extern crate foreign_inherent_extension;

fn main() {
    assert_eq!(extern_crate::StructWithAttr.marker(), 42);
}
