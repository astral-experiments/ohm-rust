use std::panic::Location;

#[track_caller]
const fn source() -> &'static str {
    Location::caller().file()
}

const CONSTANT: &str = source();

fn main() {
    struct Local;
    println!("{CONSTANT}\n{}\n{}", source(), caller_lib::source::<Local>());
}
