#![no_std]

#[unsafe(no_mangle)]
pub fn answer(input: u32) -> u32 {
    input.wrapping_add(17)
}
