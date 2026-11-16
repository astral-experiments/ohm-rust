#[inline(never)]
pub fn retain(value: u64) -> u64 {
    unsafe {
        core::arch::asm!("/* {value} */", value = in(reg) value, options(nomem, nostack));
    }
    value
}
