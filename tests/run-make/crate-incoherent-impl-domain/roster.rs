#![feature(rustc_attrs)]
#![allow(internal_features)]

pub fn anchor() {}

#[cfg(unrelated)]
pub struct Unrelated;

#[cfg(string)]
impl str {
    #[rustc_allow_incoherent_impl]
    pub fn domain_probe() -> u8 {
        11
    }
}

#[cfg(string_argument)]
impl str {
    #[rustc_allow_incoherent_impl]
    pub fn domain_probe(_value: u8) -> u8 {
        17
    }
}

#[cfg(shared)]
impl<T: ?Sized> &T {
    #[rustc_allow_incoherent_impl]
    pub fn domain_probe() -> u8 {
        23
    }
}

#[cfg(token)]
impl owner::Token {
    #[rustc_allow_incoherent_impl]
    pub fn domain_probe() -> u8 {
        31
    }
}
