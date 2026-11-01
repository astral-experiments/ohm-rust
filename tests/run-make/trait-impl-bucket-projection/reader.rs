extern crate roster;

use roster::{Destination, Kept, Score};

fn main() {
    println!("{} {}", Kept::score(), Destination::score());
}
