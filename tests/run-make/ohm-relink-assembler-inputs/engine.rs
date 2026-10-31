pub fn payload<T>() -> u32 {
    let word: u32;
    unsafe {
        core::arch::asm!(
            "jmp 3f",
            "2:",
            ".incbin \"payload.bin\"",
            ".include \"indirect.s\"",
            "3:",
            "movzx {word:e}, word ptr [rip + 2b]",
            word = out(reg) word,
            options(nostack, preserves_flags),
        );
    }
    word
}
