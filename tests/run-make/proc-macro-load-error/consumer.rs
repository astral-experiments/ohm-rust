extern crate macros;

macros::make_answer!();

fn main() {
    assert_eq!(ANSWER, 42);
}
