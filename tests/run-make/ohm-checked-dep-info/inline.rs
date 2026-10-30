#![crate_type = "rlib"]

#[cfg(global_macro)]
core::arch::global_asm!(".macro nop", ".incbin \"payload.bin\"", ".endm");

pub fn barrier(mut value: usize) -> usize {
    unsafe {
        core::arch::asm!("/* {0} */", inout(reg) value, options(pure, nomem, nostack));
        core::arch::asm!("nop");
        #[cfg(target_arch = "x86_64")]
        {
            core::arch::asm!(
                "# {}",
                in(reg) core::ptr::from_ref(&value),
                options(readonly, preserves_flags, nostack),
            );
            #[cfg(hash_count)]
            core::arch::asm!("# {} {}", in(reg) value, in(reg) value);
            #[cfg(hash_literal)]
            core::arch::asm!("# @");
            #[cfg(hash_symbol)]
            core::arch::asm!("# {}", sym barrier);
            #[cfg(hash_directive)]
            core::arch::asm!("# {}", ".incbin \"payload.bin\"", in(reg) value);
        }
    }
    value
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn divide_wide(lo: usize, hi: usize, divisor: usize) -> (usize, usize) {
    let (quotient, remainder);
    core::arch::asm!(
        "div {0:r}",
        in(reg) divisor, inout("ax") lo => quotient, inout("dx") hi => remainder,
        options(pure, nomem, nostack),
    );
    #[cfg(div_literal)]
    core::arch::asm!("div rcx", in("cx") divisor, inout("ax") lo => _, inout("dx") hi => _);
    #[cfg(div_memory)]
    core::arch::asm!(
        "div qword ptr [{}]",
        in(reg) core::ptr::from_ref(&divisor), inout("ax") lo => _, inout("dx") hi => _,
    );
    #[cfg(div_directive)]
    core::arch::asm!(
        "div {0:r}", ".incbin \"payload.bin\"",
        in(reg) divisor, inout("ax") lo => _, inout("dx") hi => _,
    );
    #[cfg(div_immediate)]
    core::arch::asm!("div 7");
    #[cfg(div_count)]
    core::arch::asm!("div {}, {}", in(reg) lo, in(reg) hi);
    (quotient, remainder)
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

#[cfg(target_arch = "x86_64")]
pub unsafe fn float_registers(mut x: f64, y: f64, z: f64) -> f64 {
    core::arch::asm!(
        "sqrtsd {x}, {x}",
        "vfmadd213sd {x}, {y}, {z}",
        "vfmaddsd {x}, {x}, {y}, {z}",
        x = inout(xmm_reg) x, y = in(xmm_reg) y, z = in(xmm_reg) z,
        options(pure, nomem, nostack),
    );
    x
}

#[cfg(all(target_arch = "x86_64", float_memory))]
pub unsafe fn float_memory(source: *const f64) -> f64 {
    let value: f64;
    core::arch::asm!(
        "sqrtsd {value}, qword ptr [{source}]",
        value = out(xmm_reg) value, source = in(reg) source,
        options(readonly, nostack),
    );
    value
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
