#![crate_type = "lib"]

use std::fmt::Write;

fn hidden() {}

#[cfg(expectations)]
#[expect(warnings)]
struct NotConstructed {
    value: u8,
}

pub fn recursive() {
    recursive();
}

const BYTES: [u8; 2] = [0, 1];

pub fn update() {
    let unused = 0;
    let mut captured = 0;
    let mut set = || {
        captured = 1;
        captured = 2;
        captured
    };
    let _ = set();
    BYTES[0] = 1;
    println!("{:p}", &update);
}

#[cfg(layout_cycle)]
pub struct Cycle(<&'static Cycle as std::ops::Deref>::Target);

#[deny(unused_results)]
pub fn local_override() {
    fn answer() -> u32 {
        42
    }
    answer();
}
