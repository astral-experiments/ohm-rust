#![crate_type = "rlib"]

fn private_value() -> u32 {
    10
}

pub fn value() -> u32 {
    private_value()
}

pub fn generic<T: Default>() -> T {
    T::default()
}
