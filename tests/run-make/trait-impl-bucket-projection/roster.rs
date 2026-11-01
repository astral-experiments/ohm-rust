#![feature(specialization)]
#![allow(incomplete_features)]

pub struct Kept;
pub struct Unrelated;

#[cfg(not(retired))]
pub struct Destination;
#[cfg(retired)]
pub type Destination = Kept;

pub trait Score {
    fn score() -> u8;
}

impl<T> Score for T {
    default fn score() -> u8 {
        3
    }
}

#[cfg(unrelated)]
impl Score for Unrelated {
    fn score() -> u8 {
        5
    }
}

#[cfg(matching)]
type Target = Kept;
#[cfg(moved)]
type Target = Destination;

#[cfg(any(matching, moved))]
impl Score for Target {
    fn score() -> u8 {
        7
    }
}

#[cfg(finite)]
impl Score for u16 {
    fn score() -> u8 {
        11
    }
}

#[cfg(finite)]
impl Score for (u8, u16) {
    fn score() -> u8 {
        13
    }
}

#[cfg(finite)]
impl Score for (u8, u16, u32, u64) {
    fn score() -> u8 {
        17
    }
}
