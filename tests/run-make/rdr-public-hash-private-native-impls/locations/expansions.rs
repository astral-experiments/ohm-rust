#![crate_name = "producer"]
#![crate_type = "rlib"]
#![allow(unused_macros)]

pub mod defs {
    macro_rules! first {
        () => { const _: () = (); };
    }
    macro_rules! public {
        () => { pub struct Public { pub value: u8 } };
    }
    #[cfg(all())] first!();
    public!();
}
