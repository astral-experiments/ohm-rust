#![crate_type = "rlib"]

extern crate shared;

pub fn value() -> u64 {
    private_value()
}

#[cfg(not(remove_shared))]
fn private_value() -> u64 {
    #[cfg(change_private_body)]
    let value = 8_u64;
    #[cfg(not(change_private_body))]
    let value = 7_u64;

    shared::shared(value)
}

#[cfg(remove_shared)]
fn private_value() -> u64 {
    7
}
