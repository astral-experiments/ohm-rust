#![crate_type = "rlib"]

#[cfg_attr(cached, maker::parsed)]
#[cfg_attr(cached, derive(maker::Parsed))]
#[cfg_attr(tracked, derive(maker::Tracked))]
pub struct Marker;

#[cfg(cached)]
pub const TRACKED: u32 = 379;
#[cfg(tracked)]
pub const DERIVED_LINE: u32 = line!();

include!("late.rs");

pub fn value() -> u8 {
    7
}
