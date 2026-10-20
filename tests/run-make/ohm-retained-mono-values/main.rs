#![feature(rustc_attrs)]

fn edited() -> usize {
    1
}

#[inline(never)]
fn shared<T>() -> usize {
    std::mem::size_of::<T>()
}

#[rustc_clean(cfg = "partition_hit", loaded_from_disk = "incremental_mono_item_partitions")]
fn main() {
    println!("{}", edited() + shared::<u8>());
}
