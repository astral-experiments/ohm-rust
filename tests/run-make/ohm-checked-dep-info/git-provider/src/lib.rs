extern "C" {
    fn value() -> u8;
}

pub fn read_value() -> u8 {
    unsafe {
        core::arch::asm!("nop");
        value()
    }
}
