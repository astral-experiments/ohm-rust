//@ revisions: rpass1 rpass2
//@ compile-flags: -Z query-dep-graph -Z public-api-hash
//@ ignore-backends: gcc

#![feature(rustc_attrs)]

struct Fails<T>(T);

impl<T> Fails<T> {
    const VALUE: () = panic!("this generic function is not monomorphized");
}

fn unreachable<T>() {
    #[cfg(rpass1)]
    let _ = 1;

    #[cfg(rpass2)]
    let _ = Fails::<T>::VALUE;
}

#[rustc_clean(cfg = "rpass2", loaded_from_disk = "incremental_mono_item_partitions")]
fn unchanged() -> usize {
    1
}

fn main() {
    assert_eq!(unchanged(), 1);
}
