pub fn payload<T>() -> u32 {
    let byte: u32;
    unsafe {
        core::arch::asm!(
            "jmp 3f",
            "2:",
            ".incbin \"payload.bin\"",
            "3:",
            "movzx {byte:e}, byte ptr [rip + 2b]",
            byte = out(reg) byte,
            options(nostack, preserves_flags),
        );
    }
    byte
}
