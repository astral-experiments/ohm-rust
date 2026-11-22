#![crate_type = "lib"]

type Byte = u8;

pub const WRONG_INTEGER: Byte = 1i8;
//~^ ERROR mismatched types
pub const WRONG_TYPE: bool = 0;
//~^ ERROR mismatched types
