#![crate_name = "mono_provider"]
#![crate_type = "rlib"]

pub fn anchor() -> u32 {
    mono_defs::identity(17_u32)
}
