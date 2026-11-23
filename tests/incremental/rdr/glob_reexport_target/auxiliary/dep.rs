//@ compile-flags: -Zpublic-api-hash

pub struct First;
pub struct Second;

pub trait Number {
    fn number(self) -> u32;
}

impl Number for First {
    fn number(self) -> u32 {
        1
    }
}

impl Number for Second {
    fn number(self) -> u32 {
        2
    }
}

pub mod api {
    pub const OFFSET: u32 = 10;
    #[cfg(not(rpass2))]
    pub use super::First as Selected;
    pub use super::Number as _;
    #[cfg(rpass2)]
    pub use super::Second as Selected;
}
