#![crate_type = "rlib"]

struct Fails<T>(T);

impl<T> Fails<T> {
    const VALUE: () = panic!("required closure constant");
}

#[inline(never)]
fn trigger<T>() {
    let _ = Fails::<T>::VALUE;
}

pub fn value() -> bool {
    let callback: fn(u8) -> bool = |value| {
        #[cfg(not(fail))]
        let _ = ();

        #[cfg(fail)]
        trigger::<u8>();

        value > 0
    };

    callback(1)
}
