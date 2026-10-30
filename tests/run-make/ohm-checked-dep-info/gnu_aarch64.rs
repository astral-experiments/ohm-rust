#![crate_type = "rlib"]

#[cfg(global)]
core::arch::global_asm!(".macro qa_unseen", ".incbin \"payload.bin\"", ".endm");

#[cfg(compact_copy)]
pub unsafe fn compact_copy(x: usize) {
    core::arch::asm!("/* {value} */", value = in(reg) x, options(nomem, nostack));
}

pub unsafe fn barrier(mut x: usize, mut v: core::arch::aarch64::int32x4_t) -> usize {
    #[cfg(syscall)]
    core::arch::asm!("svc 0", in("x8") x, inlateout("x0") x, options(nostack, preserves_flags));
    #[cfg(syscall_file)]
    core::arch::asm!("svc 0", ".incbin \"payload.bin\"", in("x8") x, inlateout("x0") x);
    #[cfg(atomic_loop)]
    core::arch::asm!(
        "2:", "ldaxp {lo}, {hi}, [{ptr}]", "stlxp {ok:w}, {lo}, {hi}, [{ptr}]", "cbnz {ok:w}, 2b",
        ptr = in(reg) x, lo = out(reg) _, hi = out(reg) _, ok = out(reg) _,
        options(nostack, preserves_flags),
    );
    #[cfg(atomic_feature)]
    core::arch::asm!(
        ".arch_extension lse", "casp x6, x7, x4, x5, [{ptr}]", ptr = in(reg) x,
        out("x4") _, out("x5") _, out("x6") _, out("x7") _, options(nostack),
    );
    #[cfg(atomic_file)]
    core::arch::asm!("ldar {v}, [{v}]", ".incbin \"payload.bin\"", v = inout(reg) x, options(nostack));
    #[cfg(atomic_other_extension)]
    core::arch::asm!(".arch_extension crc", "ldar {v}, [{v}]", v = inout(reg) x, options(nostack));
    #[cfg(atomic_literal)]
    core::arch::asm!("ldar x0, [x0]", inout("x0") x, options(nostack));
    #[cfg(not(vector_only))]
    core::arch::asm!("/*{0}*/", inout(reg) x);
    #[cfg(not(general_only))]
    core::arch::asm!("/*{:v}*/", inout(vreg) v);
    #[cfg(scalar_modifier)]
    core::arch::asm!("/*{0:x}*/", inout(reg) x);
    #[cfg(vector_modifier)]
    core::arch::asm!("/*{:q}*/", inout(vreg) v);
    #[cfg(spaced)]
    core::arch::asm!("/* {0} */", inout(reg) x);
    #[cfg(input_only)]
    core::arch::asm!("/*{0}*/", in(reg) x);
    #[cfg(late)]
    core::arch::asm!("/*{0}*/", inlateout(reg) x);
    #[cfg(discarded)]
    core::arch::asm!("/*{0}*/", inout(reg) x => _);
    #[cfg(options)]
    core::arch::asm!("/*{0}*/", inout(reg) x, options(nostack));
    #[cfg(separate)]
    {
        let other: usize;
        core::arch::asm!("/*{0}*/", inout(reg) x => other);
        core::hint::black_box((x, other));
    }
    #[cfg(compact_input)]
    core::arch::asm!("/* {value} */", value = in(reg) x, options(nomem, nostack));
    #[cfg(compact_constant)]
    core::arch::asm!("/* {value} */", value = in(reg) 7usize, options(nomem, nostack));
    #[cfg(compact_modifier)]
    core::arch::asm!("/* {0:x} */", in(reg) x, options(nomem, nostack));
    #[cfg(compact_options)]
    core::arch::asm!("/* {0} */", in(reg) x, options(nomem));
    #[cfg(compact_output)]
    core::arch::asm!("/* {0} */", inout(reg) x, options(nomem, nostack));
    #[cfg(compact_two_operands)]
    core::arch::asm!("/* {0} {1} */", in(reg) x, in(reg) x, options(nomem, nostack));
    #[cfg(compact_file)]
    core::arch::asm!("/* {0} */", ".incbin \"payload.bin\"", in(reg) x, options(nomem, nostack));
    #[cfg(instruction)]
    core::arch::asm!("nop");
    #[cfg(file)]
    core::arch::asm!("/*{:v}*/", ".incbin \"payload.bin\"", inout(vreg) v);
    #[cfg(escape)]
    core::arch::asm!("/*{0}*/ .incbin \"payload.bin\" /*trailer*/", inout(reg) x);
    #[cfg(raw)]
    core::arch::asm!("/*x0*/", options(raw));
    core::hint::black_box(v);
    x
}
