extern crate table;

use table::TABLE;

const LAST: &str = TABLE[TABLE.len() - 1].1[1];

fn main() {
    assert_eq!(TABLE[0], (&["key"][..], &["value"][..]));
    assert!(TABLE.last().unwrap().0.is_empty());
    assert_eq!(TABLE.last().unwrap().1, &["", LAST]);
    println!("{} {LAST}", TABLE.len());
}
