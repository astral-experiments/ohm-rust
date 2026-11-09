//@ revisions: bpass1 bpass2
//@ compile-flags: -Zquery-dep-graph
#![feature(rustc_attrs)]
#![allow(internal_features)]
#![crate_type = "rlib"]

#[cfg_attr(bpass1, derive(Debug))]
#[cfg_attr(bpass2, derive(Debug))]
pub enum Choice {
    Yes(u8),
    No(u8),
}

#[cfg_attr(bpass1, derive(Debug))]
#[cfg_attr(bpass2, derive(Debug))]
pub struct Point {
    pub value: u8,
}

impl Choice {
    #[rustc_clean(cfg = "bpass2", loaded_from_disk = "typeck_root")]
    pub fn choose(value: u8) -> Self {
        Self::Yes(value)
    }
}

#[rustc_clean(cfg = "bpass2", loaded_from_disk = "typeck_root")]
pub fn get(point: Point) -> u8 {
    point.value
}
