//@ revisions: rpass1 rpass2
//@ compile-flags: -Zohm-incremental-mono-partitions -Zincremental-verify-ich
//@ ignore-backends: gcc

#![allow(dead_code)]

fn main() {
    #[cfg(rpass1)]
    callee();
}

fn callee() {
    // The edited caller has removed its only call to this edited body.
    #[cfg(rpass2)]
    fail::<u8>();
}

struct Fails<T>(T);

impl<T> Fails<T> {
    const VALUE: () = panic!("evaluated an unreachable constant");
}

fn fail<T>() {
    let _ = Fails::<T>::VALUE;
}
