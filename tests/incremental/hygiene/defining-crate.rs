//@ revisions: rpass1 rpass2 rpass3
//@ aux-build: defining.rs
//@ compile-flags: -Zincremental-verify-ich

extern crate defining as dep;

const N: u32 = 1;

macro_rules! value {
    () => {
        $crate::N + dep::outer!()
    };
}

fn main() {
    // `crate` in outer! and `$crate` in inner! refer to the renamed dependency.
    // `crate` in the macro_rules! expansion of at_call_site! refers to the caller.
    assert_eq!(value!(), if cfg!(rpass2) { 7 } else { 11 });
    assert_eq!(dep::at_call_site!(), N);
}
