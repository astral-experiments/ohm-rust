extern crate lib;

fn main() {
    let caller: fn() -> &'static std::panic::Location<'static> = lib::caller_location;
    println!("{}", caller().column());
}
