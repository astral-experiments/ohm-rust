#![crate_type = "rlib"]

#[cfg(opcode_macro)]
core::arch::global_asm!(".macro lsl d, s, n", ".include \"integer-outer.s\"", ".endm");

pub unsafe fn value<const SHIFT: u32>(count: u32, base: *const u8, index: usize) -> u32 {
    let mut value: u32;
    core::arch::asm!(
        "// A directive here is a comment: .include \"absent.s\"",
        "mov {v:w}, #{shift}", "2:",
        "ldrb {v:w}, [{base}, {index}]", "cmp {count:w}, {v:w}", "b.hs 3f",
        "lsl {v:w}, {v:w}, #{shift}", "orr {v:w}, {v:w}, #1", "3:",
        "csel {v:w}, {count:w}, {v:w}, hs", "subs {count:w}, {count:w}, #1", "b.ne 2b",
        v = out(reg) value, count = inout(reg) count => _,
        base = in(reg) base, index = in(reg) index,
        shift = const SHIFT, options(pure, readonly, nostack)
    );
    #[cfg(directive)]
    core::arch::asm!("lsl {v:w}, {v:w}, #{s}", ".include \"integer-outer.s\"",
        v = inout(reg) value, s = const 1, options(pure, readonly, nostack));
    #[cfg(comment_cr)]
    core::arch::asm!("// comment\r.include \"integer-outer.s\"", "lsl {v:w}, {v:w}, #{s}",
        v = inout(reg) value, s = const 1, options(pure, readonly, nostack));
    #[cfg(spliced_immediate)]
    core::arch::asm!("lsl {v:w}, {v:w}, #{s}+1",
        v = inout(reg) value, s = const 1, options(pure, readonly, nostack));
    value
}

pub unsafe fn run(count: u32, base: *const u8, index: usize) -> u32 {
    value::<5>(count, base, index)
}
