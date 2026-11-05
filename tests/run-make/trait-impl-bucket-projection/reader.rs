extern crate roster;

use roster::{Destination, Kept, Score};

fn main() {
    struct Local;
    assert_eq!(Local::score(), 3);

    println!(
        "{} {} {} {} {}",
        Kept::score(),
        Destination::score(),
        u16::score(),
        <(u8, u16)>::score(),
        <(u8, u16, u32, u64)>::score(),
    );
}
