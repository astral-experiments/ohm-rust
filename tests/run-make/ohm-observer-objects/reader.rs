extern crate foreign;
extern crate observer;

foreign::read_value!();

#[cfg(asm_input)]
core::arch::global_asm!(
    ".pushsection .rodata",
    ".globl observer_payload",
    "observer_payload:",
    ".incbin \"payload\"",
    ".popsection",
);

pub fn payload() -> u8 {
    #[cfg(asm_input)]
    unsafe {
        extern "C" {
            static observer_payload: u8;
        }
        observer_payload
    }
    #[cfg(not(asm_input))]
    {
        0
    }
}
