//@ revisions: bpass1 bpass2 bfail3 bfail4
//@ compile-flags: -Zquery-dep-graph -Zincremental-ignore-spans -Zthreads=1

#![crate_type = "lib"]
#![feature(rustc_attrs)]
#![allow(internal_features)]

struct Number;

// The same impl can have a different associated type even if selection returns
// the same DefId. Each caller also depends on the associated type.
#[cfg(bfail4)]
type Sum = bool;
#[cfg(not(bfail4))]
type Sum = ();

macro_rules! add {
    ($rhs:ty) => {
        impl std::ops::Add<$rhs> for Number {
            type Output = Sum;
            fn add(self, _: $rhs) -> Sum { Default::default() }
        }
    };
}

add!(bool);
add!(u8);
// A second non-integer impl changes the candidate list. The literal still infers u8.
#[cfg(any(bpass2, bfail3, bfail4))]
add!(char);
// Two integer impls make selection ambiguous. Both callers now fall back to i32.
#[cfg(bfail3)]
add!(u16);

pub fn first() {
    let _: () = Number + 1;
    //[bfail3]~^ ERROR cannot add `i32` to `Number`
    //[bfail4]~^^ ERROR mismatched types
}

#[rustc_clean(cfg = "bpass2", loaded_from_disk = "typeck_root")]
pub fn second() {
    let _: () = Number + 1;
    //[bfail3]~^ ERROR cannot add `i32` to `Number`
    //[bfail4]~^^ ERROR mismatched types
}
