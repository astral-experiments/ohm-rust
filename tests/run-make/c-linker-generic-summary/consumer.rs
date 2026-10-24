extern crate producer;

#[unsafe(no_mangle)]
pub extern "C" fn api() -> u32 {
    producer::force_instantiations()
}
