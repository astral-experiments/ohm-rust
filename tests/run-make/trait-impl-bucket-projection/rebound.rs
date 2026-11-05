extern crate left;
extern crate right;
extern crate roster;

use roster::Score;

fn main() {
    println!("{} {}", left::Peer::score(), right::Peer::score());
}
