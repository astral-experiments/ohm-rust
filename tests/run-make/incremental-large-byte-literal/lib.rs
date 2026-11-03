#![crate_name = "producer"]
#![crate_type = "rlib"]

use std::mem::MaybeUninit;

pub const N: usize = include_bytes!("asset").len();
pub const CONTENT: &[u8; N] = include_bytes!("asset");
pub static LEFT: [u8; N] = *CONTENT;
pub static RIGHT: [u8; N] = *CONTENT;
pub static mut WRITABLE: [u8; N] = *CONTENT;
pub static mut OTHER_WRITABLE: [u8; N] = *CONTENT;

#[repr(C, align(64))]
pub struct Record {
    pub bytes: [u8; N],
    pub selected: &'static u8,
}

pub static FIRST: u8 = 19;
pub static SECOND: u8 = 83;

pub const RECORD: &Record =
    &Record { bytes: *CONTENT, selected: if CONTENT[N / 2] == b'a' { &FIRST } else { &SECOND } };

const fn checked_middle() -> u8 {
    let mut array = [MaybeUninit::new(0_u8); N];
    if CONTENT[N / 2] == b'!' {
        array[N / 2] = MaybeUninit::uninit();
    }
    unsafe { array[N / 2].assume_init() }
}

pub const CHECKED: u8 = checked_middle();
