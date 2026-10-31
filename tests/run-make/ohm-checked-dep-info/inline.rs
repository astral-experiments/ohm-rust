#![crate_type = "rlib"]

#[cfg(global_macro)]
core::arch::global_asm!(".macro nop", ".incbin \"payload.bin\"", ".endm");

pub fn barrier(mut value: usize) -> usize {
    unsafe {
        core::arch::asm!("/* {0} */", inout(reg) value, options(pure, nomem, nostack));
        core::arch::asm!("nop");
    }
    value
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn syscall(number: usize) -> usize {
    let result: usize;
    core::arch::asm!(
        "syscall",
        inlateout("rax") number => result, lateout("rcx") _, lateout("r11") _,
    );
    // Its inline body moves a scratch register around cpuid.
    result ^ core::arch::x86_64::__cpuid_count(0, 0).eax as usize
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn atomic_ops(address: *mut u64) {
    core::arch::asm!(
        "lock not qword ptr [{address}]",
        "mov rax, qword ptr [{address} + 8]",
        "cmp rax, 0",
        "jne 2f",
        "vmovdqa {vector}, xmmword ptr [{address}]",
        "2:",
        address = in(reg) address,
        vector = out(xmm_reg) _,
        out("rax") _,
        options(nostack),
    );
}

#[cfg(label_escape)]
pub fn label_reads_file() {
    unsafe { core::arch::asm!("2: .incbin \"payload.bin\"") }
}

#[cfg(comment_escape)]
pub fn reads_file() {
    unsafe { core::arch::asm!(concat!("/* first */ .inc", "bin \"payload.bin\" /* last */")) }
}

#[cfg(symbol)]
pub fn symbol_operand() {
    unsafe { core::arch::asm!("/* {0} */", sym barrier) }
}

#[cfg(constant)]
pub fn const_operand() {
    unsafe { core::arch::asm!("/* {0} */", const 42) }
}

#[cfg(raw)]
pub fn raw_template() {
    unsafe { core::arch::asm!("nop", options(raw)) }
}
