#![feature(rustc_attrs)]
#![allow(internal_features)]

use subject::{Error, Lock};

#[rustc_clean(cfg = "moved", loaded_from_disk = "typeck_root")]
fn convert(value: Result<(), u8>) -> Result<(), Error> {
    Ok(value?)
}

#[rustc_clean(cfg = "moved")]
fn total(lock: &Lock) -> u32 {
    let cutoff = String::from("x");
    let view = lock.auditable(move |n| usize::from(*n) > cutoff.len());
    view.packages()
        .map(|(a, b)| {
            let a: &u8 = a;
            let b: &u16 = b;
            u32::from(*a) + u32::from(*b)
        })
        .sum()
}

#[cfg(bad_filter)]
fn wrong(lock: &Lock) {
    let accepts_u16 = |_: &u16| true;
    let _ = lock.auditable(accepts_u16);
}

fn main() {
    let lock = Lock(vec![(3, 40), (11, 2)]);
    println!("{} {}", convert(Err(37)).err().unwrap().0, total(&lock));
}
