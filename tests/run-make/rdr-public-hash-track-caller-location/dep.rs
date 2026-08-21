#![crate_name = "dep"]
#![crate_type = "rlib"]

#[track_caller]
pub fn location() -> (&'static str, u32, u32) {
    let location = std::panic::Location::caller();
    (location.file(), location.line(), location.column())
}
