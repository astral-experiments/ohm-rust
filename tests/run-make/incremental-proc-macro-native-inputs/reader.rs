#[macro_use]
extern crate native;

#[derive(Observe)]
struct Marker;

#[observe_attr]
const ATTR: i32 = 0;

fn main() {
    let _ = Marker;
    println!("{DERIVED},{ATTR}");
}
