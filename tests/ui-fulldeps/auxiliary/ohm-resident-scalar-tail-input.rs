#![allow(dead_code)]

macro_rules! forward {
    ($value:expr) => { $value };
}

fn identity<T>(value: T) -> T {
    value
}

fn free_call(value: bool) -> bool {
    identity(value)
}

struct Example<'a>(&'a str);

impl<'a> Example<'a> {
    fn method_call(&self, value: u32) -> char {
        char::from_u32(value).unwrap()
    }

    fn explicit_method<'b>(&'b self, value: u32) -> char {
        char::from_u32(value).unwrap()
    }
}

fn forwarded(value: bool) -> bool {
    forward!(value)
}

fn explicit_generic(value: bool) -> bool {
    identity::<bool>(value)
}

fn explicit_lifetime<'a>(_value: &'a bool) -> bool {
    identity(true)
}

fn closure(value: bool) -> bool {
    (|| value)()
}

pub fn public_call(value: bool) -> bool {
    identity(value)
}

fn untouched() -> bool {
    false
}

fn stacked() -> bool {
    true
}
