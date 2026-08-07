#![crate_type = "rlib"]

extern crate library;

pub fn value() -> usize {
    library::value()
}
