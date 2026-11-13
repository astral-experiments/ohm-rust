#[unsafe(no_mangle)]
pub fn assembly_warning() {
    unsafe {
        core::arch::asm!(r#".warning "Ohm object cache warning""#);
    }
}
