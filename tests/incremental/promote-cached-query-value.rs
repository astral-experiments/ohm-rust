//@ revisions: rpass1 rpass2 rpass3
//@ compile-flags: -Zquery-dep-graph -Ccodegen-units=1 -Zthreads=4
//@ ignore-backends: gcc

#![feature(rustc_attrs)]

// Reusing the codegen unit in rpass2 must preserve the cached MIR for rpass3,
// when the changed caller makes us compile the unit again.
#[rustc_clean(cfg = "rpass2")]
#[rustc_clean(cfg = "rpass3", loaded_from_disk = "optimized_mir")]
#[inline(never)]
fn cached(value: usize) -> usize {
    value * 2
}

fn main() {
    let value = cached(21);
    #[cfg(not(rpass3))]
    assert_eq!(value, 42);
    #[cfg(rpass3)]
    assert_eq!(value + 1, 43);
}
