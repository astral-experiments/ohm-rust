//@ run-pass
//@ edition:2024

#![feature(trait_alias)]
#![deny(unused_imports)]

mod defs {
    macro_rules! method {
        () => {
            fn value(self) -> u8;
        };
    }

    pub trait Small { method!(); }
    pub trait Large { fn value(self) -> u8; }

    impl Small for u16 { fn value(self) -> u8 { 1 } }
    impl Large for u32 { fn value(self) -> u8 { 2 } }

    pub trait Alias = Small;
}

mod both {
    pub use crate::defs::{Large, Small as _};
}

fn main() {
    {
        use both::*;
        assert_eq!(8u16.value(), 1);
        assert_eq!(8u32.value(), 2);
        assert_eq!(u16::value(8), 1);
    }
    {
        // An alias has no item list in name resolution.
        use defs::Alias;
        assert_eq!(16u16.value(), 1);
    }
}
