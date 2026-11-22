//@ revisions: rpass1 rpass2 rpass3
//@ aux-build: defs.rs

extern crate defs;

// Exercise constant evaluation, promotion and pattern matching in the consumer.
const WORD: &defs::Word = &defs::WORD;
const DATA: [u8; defs::COUNT] = [0; defs::COUNT];

fn main() {
    assert_eq!(std::mem::size_of_val(WORD), if cfg!(rpass1) { 1 } else { 2 });
    assert_eq!(*WORD, 255);
    assert_eq!(DATA.len(), if cfg!(rpass3) { 8 } else { 7 });
    assert!(matches!(i8::MAX, defs::MAX));
    assert_eq!(defs::WIDE, u128::MAX);
}
