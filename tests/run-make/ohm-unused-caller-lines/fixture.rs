#[track_caller]
#[inline(never)]
fn ignore(x: u64) -> u64 {
    x.rotate_left(5)
}

pub fn call(x: u64) -> u64 {
    ignore(x)
}
