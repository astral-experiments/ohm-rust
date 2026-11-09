#![crate_type = "rlib"]

pub fn sample() -> u16 {
    37
}

#[macro_export]
macro_rules! inner {
    () => {{ $crate::sample() }};
    ($value:expr) => {{ $value }};
}

#[macro_export]
macro_rules! origin {
    () => {
        $crate::inner!()
    };
    ($value:expr) => {
        $crate::inner!($value)
    };
}
