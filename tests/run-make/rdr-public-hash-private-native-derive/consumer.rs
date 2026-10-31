extern crate dep;

fn main() {
    let value = dep::inferred!();
    assert!(value == value);
    println!("{}", std::mem::size_of_val(&value));
}
