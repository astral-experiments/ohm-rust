//@ compile-flags: --crate-type=lib

#![feature(no_core)]
#![no_core]

struct BitsImpl<const N: usize>;
type Chosen = BitsImpl<7>; //~ ERROR requires `sized` lang_item
