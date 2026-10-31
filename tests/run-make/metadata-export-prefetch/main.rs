extern crate provider;

fn main() {
    println!("{} {} {}", provider::measured(), provider::measure(3u16), provider::measure(3u8));
}
