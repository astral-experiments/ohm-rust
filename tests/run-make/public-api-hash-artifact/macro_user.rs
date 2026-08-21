extern crate lib;
extern crate macro_inspector;

use macro_inspector::observe;

fn main() {
    println!("{}", lib::forward!(observe));
}
