//@ normalize-stderr: "0x[0-9a-f]+ │.*" -> "HEX_DUMP"
//@ dont-require-annotations: NOTE

use std::mem::{MaybeUninit, transmute};

const SURROGATE: [char; 16] = unsafe {
    //~^ ERROR invalid value of type [char; 16]: at [15]
    let mut chars = [0u32; 16];
    chars[15] = 0xd800;
    transmute(chars)
};

const OUT_OF_RANGE: [(char, char); 16] = unsafe {
    //~^ ERROR invalid value of type [(char, char); 16]: at [7].1
    let mut pairs = [(0u32, 0u32); 16];
    pairs[7].1 = 0x11_0000;
    transmute(pairs)
};

const UNINIT: [char; 16] = unsafe {
    //~^ ERROR invalid value of type [char; 16]: at [9]
    let mut chars = [MaybeUninit::new('x'); 16];
    chars[9] = MaybeUninit::uninit();
    transmute(chars)
};

#[repr(C)]
union Storage {
    ptr: &'static i32,
    chars: [char; 16],
}

const POINTER: [char; 16] = unsafe {
    //~^ ERROR invalid value of type [char; 16]: at [0]
    let mut storage = Storage { chars: ['a'; 16] };
    storage.ptr = &42;
    storage.chars
};

fn main() {}
