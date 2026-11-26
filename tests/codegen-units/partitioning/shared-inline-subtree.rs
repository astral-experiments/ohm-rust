//@ incremental
//@ compile-flags: -Copt-level=0

#![crate_type = "rlib"]

// The roots in a share a subtree. Reaching b's root from a must not bring
// b's other private copy along, and b must still get its own shared subtree.
#[inline(always)]
//~ MONO_ITEM fn shared @@ shared_inline_subtree-a[Internal] shared_inline_subtree-b[Internal]
fn shared(x: i32) -> i32 {
    leaf(x)
}

#[inline(always)]
//~ MONO_ITEM fn leaf @@ shared_inline_subtree-a[Internal] shared_inline_subtree-b[Internal]
fn leaf(x: i32) -> i32 {
    !x
}

pub mod a {
    //~ MONO_ITEM fn a::first @@ shared_inline_subtree-a[External]
    pub fn first(x: i32) -> i32 {
        super::shared(x)
    }

    //~ MONO_ITEM fn a::second @@ shared_inline_subtree-a[External]
    pub fn second(x: i32) -> i32 {
        super::shared(super::b::other(x))
    }
}

pub mod b {
    #[inline(always)]
    //~ MONO_ITEM fn b::private @@ shared_inline_subtree-b[Internal]
    fn private(x: i32) -> i32 {
        super::shared(x)
    }

    #[inline(never)]
    //~ MONO_ITEM fn b::other @@ shared_inline_subtree-b[External]
    pub fn other(x: i32) -> i32 {
        private(x)
    }
}
