#[cfg(rpass1)]
pub type Word = u8;
#[cfg(not(rpass1))]
pub type Word = u16;

// Only the alias changes between the first two revisions.
pub const WORD: Word = 255;

#[cfg(not(rpass3))]
macro_rules! count {
    () => {
        7
    };
}
#[cfg(rpass3)]
macro_rules! count {
    () => {
        8
    };
}

pub const COUNT: usize = count!();
pub const MAX: i8 = 127i8;
pub const WIDE: u128 = 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF;
