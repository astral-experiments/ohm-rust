#![allow(dead_code)]
#![crate_type = "rlib"]

#[cfg(any())]
mod additional;
pub mod defs;

struct Private;

impl Iterator for Private {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}
