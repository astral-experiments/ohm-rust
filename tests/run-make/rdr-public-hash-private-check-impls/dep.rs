#![crate_type = "rlib"]
#![allow(dead_code)]

mod hidden;

pub fn number() -> impl Copy {
    1u8
}

#[derive(Clone, Copy)]
struct Guard;

pub struct Handle(Guard);

#[cfg(exposed)]
pub fn guard() -> impl Copy {
    Guard
}

#[cfg(exposed)]
pub fn state() -> impl Clone {
    hidden::State::from(0u8)
}
