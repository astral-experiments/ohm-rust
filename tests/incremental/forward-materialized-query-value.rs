//@ revisions: rpass1 rpass2 rpass3
//@ compile-flags: -Zquery-dep-graph -Ccodegen-units=1 -Zincremental-verify-ich
//@ ignore-backends: gcc

#![feature(rustc_attrs)]

// rpass1 saves cached's MIR; changing main makes codegen load it in rpass2.
// Promotion visits the saved index even though the value is now in memory.
// rpass3 loads the value again after the cache has been serialized.
#[rustc_clean(cfg = "rpass2", loaded_from_disk = "optimized_mir")]
#[rustc_clean(cfg = "rpass3", loaded_from_disk = "optimized_mir")]
#[inline(never)]
fn cached(value: usize) -> usize {
    value * 2
}

fn main() {
    let value = cached(21);
    #[cfg(rpass1)]
    assert_eq!(value, 42);
    #[cfg(rpass2)]
    assert_eq!(value + 1, 43);
    #[cfg(rpass3)]
    assert_eq!(value + 2, 44);
}
