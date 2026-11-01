pub mod left;
pub mod right;

pub static FIRST: u8 = 17;
pub static SECOND: u8 = 17;
pub const POINTERS: [&u8; 3] = [&FIRST, &FIRST, &SECOND];
