extern crate dep;

#[unsafe(no_mangle)]
pub extern "C" fn answer() -> u32 {
    dep::value() + 1
}
