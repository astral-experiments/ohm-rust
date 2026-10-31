pub struct Token(pub u32);

#[inline(never)]
pub fn value() -> u32 {
    if cfg!(first) { 1 } else { 2 }
}
