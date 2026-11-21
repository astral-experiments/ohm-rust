#![feature(rustc_attrs)]
#![allow(internal_features)]

mod packet {
    #[repr(C)]
    pub struct Packet {
        #[cfg(not(renamed))]
        pub first: u32,
        #[cfg(all(renamed, not(swapped)))]
        pub second: u32,
        #[cfg(not(any(removed, hidden)))]
        pub data: u32,
        #[cfg(hidden)]
        data: u32,
        #[cfg(removed)]
        pub other: u32,
        #[cfg(swapped)]
        pub second: u32,
    }
}

pub use packet::Packet;

#[rustc_clean(cfg = "reuse", loaded_from_disk = "typeck_root,mir_borrowck")]
pub const fn read(packet: &Packet) -> u32 {
    packet.data
}

#[cfg(all(swapped, not(any(removed, hidden))))]
const _: () = assert!(read(&Packet { data: 7, second: 42 }) == 7);
