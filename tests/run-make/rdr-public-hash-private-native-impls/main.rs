extern crate dep;

fn main() {
    println!("{} {}", dep::layout(()), dep::value());
}
