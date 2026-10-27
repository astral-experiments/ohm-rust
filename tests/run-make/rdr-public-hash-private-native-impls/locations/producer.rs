#![allow(dead_code)]
#![crate_type = "rlib"]

#[cfg(any())]
mod extra;
pub mod defs;

struct Private;

impl Iterator for Private {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}
