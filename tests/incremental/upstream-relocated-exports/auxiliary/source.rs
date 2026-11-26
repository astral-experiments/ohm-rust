//@ no-prefer-dynamic
//@ compile-flags: -Zshare-generics=yes

#![crate_type = "rlib"]

#[inline(never)]
pub fn number<T, const N: u32>() -> u32 {
    N
}

pub fn all() -> u32 {
    // Enough shared instances to get indexed metadata. The repeated type
    // exercises shorthand references outside the newly requested records.
    macro_rules! sum {
        ($base:expr; $($n:literal),*) => {
            0 $(+ number::<[u32; 3], { $base + $n }>())*
        };
    }
    macro_rules! blocks {
        ($($b:literal),*) => {
            0 $(+ sum!($b * 16; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15))*
        };
    }
    blocks!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15)
}

#[cfg(rpass2)]
#[allow(dead_code)]
fn private() {}
