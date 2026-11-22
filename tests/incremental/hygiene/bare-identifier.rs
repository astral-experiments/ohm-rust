//@ revisions: rpass1 rpass2 rpass3
//@ compile-flags: -Zincremental-verify-ich

#![allow(dead_code, unused_parens)]

mod left {
    pub const VALUE: u32 = 1;
    pub type Word = u8;
}

mod right {
    pub const VALUE: u32 = 2;
    pub type Word = u16;
}

#[cfg(any(rpass1, rpass3))]
use left::{VALUE, Word};
#[cfg(rpass2)]
use right::{VALUE, Word};

// Only the resolution changes. Both the name and its span stay the same.
fn bare() -> u32 {
    VALUE
}

// Only the type argument's resolution changes in this body.
fn width() -> usize {
    use std::num::NonZero;
    std::mem::size_of::<NonZero<Word>>()
}

// A parenthesized expression has a wider span than its path segment.
fn parenthesized() -> u32 {
    (VALUE)
}

fn hygienic(value: u32) -> u32 {
    macro_rules! original {
        () => { value };
    }
    let value = VALUE;
    original!() + value
}

fn main() {
    let expected = if cfg!(rpass2) { 2 } else { 1 };
    assert_eq!(bare(), expected);
    assert_eq!(width(), expected as usize);
    assert_eq!(parenthesized(), expected);
    assert_eq!(hygienic(10), 10 + expected);
}
