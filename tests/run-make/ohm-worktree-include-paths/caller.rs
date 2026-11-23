use std::panic::Location;

#[track_caller]
const fn source() -> &'static str {
    Location::caller().file()
}

const CONSTANT: &str = source();

fn main() {
    struct Local;
    assert_eq!(observe::filename!(), CONSTANT);
    println!("{CONSTANT}\n{}\n{}", source(), caller_lib::source::<Local>());
}
