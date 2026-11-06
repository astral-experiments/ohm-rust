extern crate roster;

trait Fallback {
    fn domain_probe() -> u8 {
        0
    }
}

impl Fallback for str {}
impl<T: ?Sized> Fallback for &T {}
impl Fallback for owner::Token {}

fn read_str() -> u8 {
    <str>::domain_probe()
}
fn read_reference() -> u8 {
    <&u8>::domain_probe()
}
fn read_adt() -> u8 {
    owner::Token::domain_probe()
}

fn main() {
    roster::anchor();
    println!("{} {} {}", read_str(), read_reference(), read_adt());
}
