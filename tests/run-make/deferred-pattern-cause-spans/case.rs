#![crate_type = "lib"]

pub struct Packet<T>(pub T);

#[cfg(pattern)]
pub fn check_origin() {
    match pattern_support::origin!() {
        Some(_) => {}
        _ => {}
    }
}

#[cfg(region)]
pub fn retain_origin<'a>(input: Packet<&'a u8>) {
    let Packet::<&'static u8>(_retained) = pattern_support::origin!(input);
}
