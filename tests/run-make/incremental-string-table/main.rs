extern crate table;

use table::{MIXED, TABLE};

const LAST: &str = TABLE[TABLE.len() - 1].1[1];

const FIRST: char = MIXED[0].2[1];

fn main() {
    assert_eq!(FIRST, 'λ');
    assert_eq!(MIXED[0], (1, 'a', &['A', 'λ'][..]));
    assert_eq!(MIXED[MIXED.len() - 1], (255, '🦀', &[][..]));
    assert_eq!(TABLE[0], (&["key"][..], &["value"][..]));
    assert_eq!(TABLE[TABLE.len() - 2].1, &["a", "b"]);
    assert!(TABLE.last().unwrap().0.is_empty());
    assert_eq!(TABLE.last().unwrap().1, &["", LAST]);
    println!("{} {LAST}", TABLE.len());
}
