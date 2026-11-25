use up::Block;

#[inline(never)]
#[unsafe(no_mangle)]
pub fn select_tail(a: &Block, b: &Block, enabled: bool) -> u8 {
    if enabled { a.tail().wrapping_add(b.tail()) } else { 0 }
}

fn main() {
    let a = Block::new(42);
    let mut b = Block::new(43);
    assert!(!a.compare_blocks(&b));
    assert_eq!(select_tail(&a, &b, true), 85);
    assert_eq!(select_tail(&a, &b, false), 0);
    assert_eq!(up::c_tail(&a), 42);
    up::unrounded(&mut b, &up::Public(a), &Block::new(5).generic());
    assert_eq!(select_tail(&b, &b, true), 94);
    println!("{}", size_of::<Block>());
}
