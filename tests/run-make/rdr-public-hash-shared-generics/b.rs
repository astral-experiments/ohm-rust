#![crate_type = "rlib"]

extern crate a;
extern crate shared;

pub fn value() -> u64 {
    let _ = a::value();
    shared::shared(7_u64)
}
