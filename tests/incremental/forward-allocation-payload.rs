//@ revisions: rpass1 rpass2 rpass3 rpass4 rpass5 rpass6 rpass7
//@ compile-flags: -Zquery-dep-graph -Ccodegen-units=1 -Zthreads=4 -Zincremental-verify-ich
//@ ignore-backends: gcc

#![feature(rustc_attrs)]

fn increment(value: usize) -> usize {
    value + 1
}

macro_rules! payload {
    () => {
        (&[0x5a; 4096], increment, &["nested", "allocation"])
    };
}

// Every changed caller loads this MIR again, including allocations that were
// already retained in another generation. Exercise two reuse/compaction cycles
// with memory, function, and nested-reference allocations and macro hygiene.
#[rustc_clean(cfg = "rpass2", loaded_from_disk = "optimized_mir")]
#[rustc_clean(cfg = "rpass3", loaded_from_disk = "optimized_mir")]
#[rustc_clean(cfg = "rpass4", loaded_from_disk = "optimized_mir")]
#[rustc_clean(cfg = "rpass5", loaded_from_disk = "optimized_mir")]
#[rustc_clean(cfg = "rpass6", loaded_from_disk = "optimized_mir")]
#[rustc_clean(cfg = "rpass7", loaded_from_disk = "optimized_mir")]
#[inline(never)]
fn cached() -> (&'static [u8], fn(usize) -> usize, &'static [&'static str]) {
    payload!()
}

fn main() {
    let (bytes, function, strings) = cached();
    assert_eq!(bytes, &[0x5a; 4096]);
    assert_eq!(strings, &["nested", "allocation"]);
    let value = function(41);
    #[cfg(rpass1)]
    assert_eq!(value, 42);
    #[cfg(rpass2)]
    assert_eq!(value + 1, 43);
    #[cfg(rpass3)]
    assert_eq!(value + 2, 44);
    #[cfg(rpass4)]
    assert_eq!(value + 3, 45);
    #[cfg(rpass5)]
    assert_eq!(value + 4, 46);
    #[cfg(rpass6)]
    assert_eq!(value + 5, 47);
    #[cfg(rpass7)]
    assert_eq!(value + 6, 48);
}
