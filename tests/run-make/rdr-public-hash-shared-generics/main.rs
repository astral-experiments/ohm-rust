extern crate a;
extern crate b;
extern crate shared;

fn main() {
    assert_eq!(b::value(), 7);

    #[cfg(expect_changed)]
    assert_eq!(a::value(), 8);

    #[cfg(not(expect_changed))]
    assert_eq!(a::value(), 7);
}
