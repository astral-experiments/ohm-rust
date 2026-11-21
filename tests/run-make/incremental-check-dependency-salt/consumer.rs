#![feature(rustc_attrs)]
#![allow(internal_features)]

use supplier::Packet;

#[rustc_clean(cfg = "release", loaded_from_disk = "fn_sig")]
pub fn value(packet: Packet) -> u8 {
    packet.value
}

pub const OFFSET: usize = std::mem::offset_of!(Packet, value);
pub fn header() -> [u8; OFFSET] {
    [0; OFFSET]
}
pub const TYPE_ID: std::any::TypeId = std::any::TypeId::of::<Packet>();

pub fn pair(_: &dyn supplier::Pair<First = u8, Second = u16>) {}
