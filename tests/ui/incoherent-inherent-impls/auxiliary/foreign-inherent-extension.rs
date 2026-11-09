//@ compile-flags: -Zpublic-api-hash
#![feature(rustc_attrs)]
#![allow(internal_features)]

extern crate extern_crate;

impl extern_crate::StructWithAttr {
    #[rustc_allow_incoherent_impl]
    pub fn marker(&self) -> u32 {
        42
    }
}
