#[track_caller]
#[inline(never)]
fn ignore(x: u64) -> u64 {
    x.rotate_left(5)
}

#[track_caller]
#[inline(never)]
fn location() -> u32 {
    std::panic::Location::caller().line()
}

macro_rules! both {
    ($x:expr) => {
        (ignore($x), location())
    };
}

pub fn call(x: u64) -> (u64, u32) {
    both!(x)
}
