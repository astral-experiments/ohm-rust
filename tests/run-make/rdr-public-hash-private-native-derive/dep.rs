#![allow(dead_code)]
#![crate_type = "rlib"]

mod rules {
    mod hidden;

    #[cfg(exposed)]
    pub fn make() -> impl Copy + PartialEq {
        hidden::Hidden::Value(7)
    }
}

pub fn unrelated() -> impl Copy {
    7u8
}

#[macro_export]
macro_rules! unrelated {
    () => {
        $crate::unrelated()
    };
}

#[cfg(exposed)]
pub use rules::make;

#[cfg(exposed)]
#[macro_export]
macro_rules! inferred {
    () => {
        $crate::make()
    };
}
