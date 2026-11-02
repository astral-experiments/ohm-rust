#![crate_type = "rlib"]

extern crate producer;

pub fn observed() -> (u8, u32, &'static str) {
    assert_eq!(producer::TRACKED, 379);
    (producer::value(), producer::DERIVED_LINE, producer::position())
}
