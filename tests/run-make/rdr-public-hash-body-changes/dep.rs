#![crate_name = "dep"]
#![crate_type = "rlib"]

#[macro_export]
macro_rules! exported_value {
    () => {
        /* macro */
        1usize
    };
}

pub const EXPORTED_CONST: usize = /* const */ 1;

pub fn ordinary() -> usize {
    private_body_a()
}

#[inline]
pub fn inlined() -> usize {
    /* inline */
    1
}

pub fn generic<T>() -> usize {
    /* generic */
    1
}

fn private_body_a() -> usize {
    /* private */
    1
}

fn private_body_b() -> usize {
    2
}
