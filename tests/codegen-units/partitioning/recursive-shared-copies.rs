//@ incremental
//@ compile-flags: -Copt-level=0

#![crate_type = "rlib"]

// Both roots need the whole cycle of private copies.
#[inline(always)]
//~ MONO_ITEM fn even @@ recursive_shared_copies-a[Internal] recursive_shared_copies-b[Internal]
fn even(n: usize) -> bool {
    n == 0 || odd(n - 1)
}

#[inline(always)]
//~ MONO_ITEM fn odd @@ recursive_shared_copies-a[Internal] recursive_shared_copies-b[Internal]
fn odd(n: usize) -> bool {
    n != 0 && even(n - 1)
}

pub mod a {
    //~ MONO_ITEM fn a::root @@ recursive_shared_copies-a[External]
    pub fn root(n: usize) -> bool {
        super::even(n)
    }
}

pub mod b {
    //~ MONO_ITEM fn b::root @@ recursive_shared_copies-b[External]
    pub fn root(n: usize) -> bool {
        super::odd(n)
    }
}
