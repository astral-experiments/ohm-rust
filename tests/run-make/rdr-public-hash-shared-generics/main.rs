extern crate a;
extern crate b;
extern crate shared;

fn main() {
    assert_eq!(b::value(), 7);
}
