#![feature(allow_internal_unstable)]
#![allow(internal_features)]

use std::io::ErrorKind;
use std::matches as accepts;

pub const DIRECT: bool = matches!(const { 2 }, 1 | 2);
pub const GUARDED: bool = accepts!(Some(2), Some(n) if n > BOUND,);
const _: () = assert!(DIRECT && GUARDED == EXPECTED);

// The caller does not enable the lint feature. The wrapper can still set its level.
#[allow_internal_unstable(non_exhaustive_omitted_patterns_lint)]
macro_rules! wrap {
    ($expr:expr) => {
        #[deny(non_exhaustive_omitted_patterns)]
        pub const FOUND: bool = $expr;
    };
}
wrap!(matches!(ErrorKind::NotFound, ErrorKind::NotFound));

macro_rules! matches {
    ($e:expr, $p:pat) => {
        false
    };
}
const _: () = assert!(FOUND && !matches!(0, 0));
