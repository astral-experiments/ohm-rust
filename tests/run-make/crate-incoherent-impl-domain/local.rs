#![feature(rustc_attrs)]
#![allow(internal_features)]

trait Fallback {
    fn local_domain_probe() -> u8 {
        0
    }
}
impl Fallback for u16 {}

#[cfg(local)]
impl u16 {
    #[rustc_allow_incoherent_impl]
    fn local_domain_probe() -> u8 {
        43
    }
}

struct Local;

impl Local {
    fn value(&self) -> u8 {
        17
    }
}

fn main() {
    assert_eq!(Local.value(), 17);
    println!("{}", u16::local_domain_probe());
}
