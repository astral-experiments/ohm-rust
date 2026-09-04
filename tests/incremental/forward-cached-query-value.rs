//@ revisions: rpass1 rpass2 rpass3 rpass4 rpass5 rpass6 rpass7
//@ compile-flags: -Zquery-dep-graph -Ccodegen-units=1 -Zthreads=4 -Zincremental-verify-ich
//@ ignore-backends: gcc

#![feature(rustc_attrs)]

macro_rules! retained {
    ($value:expr) => {
        ("retained allocation", $value * 2)
    };
}

// Keep the MIR unused for several revisions, then load it through the retained
// cache. Repeat after compaction, including its allocation and macro hygiene.
#[rustc_clean(cfg = "rpass2")]
#[rustc_clean(cfg = "rpass3")]
#[rustc_clean(cfg = "rpass4", loaded_from_disk = "optimized_mir")]
#[rustc_clean(cfg = "rpass5")]
#[rustc_clean(cfg = "rpass6")]
#[rustc_clean(cfg = "rpass7", loaded_from_disk = "optimized_mir")]
#[inline(never)]
fn cached(value: usize) -> (&'static str, usize) {
    retained!(value)
}

fn main() {
    let (label, value) = cached(21);
    assert_eq!(label, "retained allocation");
    #[cfg(any(rpass1, rpass2, rpass3))]
    assert_eq!(value, 42);
    #[cfg(any(rpass4, rpass5, rpass6))]
    assert_eq!(value + 1, 43);
    #[cfg(rpass7)]
    assert_eq!(value + 2, 44);
}
