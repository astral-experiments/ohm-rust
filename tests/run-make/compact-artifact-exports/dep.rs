#[link(name = "native", kind = "static")]
unsafe extern "C" {
    fn native_answer() -> i32;
}

#[inline(never)]
pub fn answer() -> i32 {
    unsafe { native_answer() + 1 }
}
