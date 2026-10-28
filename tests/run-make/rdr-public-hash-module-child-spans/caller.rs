extern crate input;

use std::panic::Location;

fn main() {
    let f: fn() -> &'static Location<'static> = input::location;
    println!("{}", f().column());
}
