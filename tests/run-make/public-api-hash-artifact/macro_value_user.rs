extern crate lib;
extern crate macro_inspector;

use macro_inspector::value;

fn main() {
    println!("{}", lib::forward!(value));
}
