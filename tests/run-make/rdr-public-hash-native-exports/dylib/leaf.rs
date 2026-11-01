mod hidden;

#[inline(never)]
pub fn read(x: u64) -> u64 {
    hidden::converted(x)
}
