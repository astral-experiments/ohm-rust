#[inline]
pub fn origin() -> (u32, u32) {
    first(1)
}

#[inline]
#[track_caller]
pub fn first(value: u32) -> (u32, u32) {
    second(value)
}

#[inline]
#[track_caller]
fn second(value: u32) -> (u32, u32) {
    observe(value)
}

#[inline(never)]
#[track_caller]
pub fn observe(value: u32) -> (u32, u32) {
    (std::panic::Location::caller().line(), value)
}
