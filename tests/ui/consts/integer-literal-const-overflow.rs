#![crate_type = "lib"]

type Byte = u8;

pub const TOO_BIG: Byte = 256;
//~^ ERROR literal out of range for `u8`
pub const TOO_POSITIVE: i8 = 128i8;
//~^ ERROR literal out of range for `i8`
