#[derive(Clone)]
pub struct Toggled(pub u8);

pub const BYTE: u8 = 7;

#[cfg(copy_impl)]
impl Copy for Toggled {}
