#[inline(never)]
fn value() -> u32 {
    if cfg!(second) { 2 } else { 1 }
}

mod other {
    #[inline(never)]
    pub fn assemble() {
        #[cfg(broken)]
        unsafe {
            std::arch::asm!("ohm_invalid_instruction");
        }
    }
}

fn main() {
    other::assemble();
    assert_eq!(value(), if cfg!(second) { 2 } else { 1 });
    #[cfg(print_value)]
    println!("{}", value());
}
