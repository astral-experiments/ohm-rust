#![allow(dead_code)]

extern crate contracts;

pub struct Item;
pub struct Extra;

#[cfg(item)]
impl contracts::Value for Item {
    fn value(&self) -> u8 {
        1
    }
}

#[cfg(extra)]
impl contracts::Value for Extra {
    fn value(&self) -> u8 {
        2
    }
}

mod hidden;
