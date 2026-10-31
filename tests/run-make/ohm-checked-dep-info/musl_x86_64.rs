#![feature(no_core)]
#![no_core]
#![crate_type = "rlib"]

extern crate minicore;
use minicore::asm;

#[cfg(global)]
minicore::global_asm!("/* no instruction */");

#[cfg(any(spaced, both))]
pub fn input(value: usize) {
    unsafe { asm!("/* {value} */", value = in(reg) value, options(nomem, nostack)) }
}

#[cfg(not(spaced))]
pub fn barrier(mut value: usize) -> usize {
    unsafe {
        asm!("/*{0}*/", inout(reg) value);

        #[cfg(instruction)]
        asm!("nop");
        #[cfg(include)]
        asm!("/*{0}*/", ".include \"payload.s\"", inout(reg) value);
        #[cfg(escape)]
        asm!("/*{0}*/ .incbin \"payload.bin\" /* end */", inout(reg) value);
        #[cfg(modifier)]
        asm!("/*{0:r}*/", inout(reg) value);
        #[cfg(class)]
        asm!("/*{0}*/", inout(reg_abcd) value);
        #[cfg(options)]
        asm!("/*{0}*/", inout(reg) value, options(nostack));
        #[cfg(symbol)]
        asm!("/*{0}*/", sym barrier);
        #[cfg(raw)]
        asm!("/*{0}*/", options(raw));
        #[cfg(att)]
        asm!("/*{0}*/", inout(reg) value, options(att_syntax));
    }
    value
}
