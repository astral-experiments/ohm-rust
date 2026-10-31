extern crate bridge;

fn main() {
    println!("{}", bridge::Public { value: 7 }.value);
}
