#[inline(never)]
fn internal_answer() -> i32 {
    #[cfg(dependency)]
    return dep::answer();
    #[cfg(not(dependency))]
    return 42;
}

#[unsafe(no_mangle)]
pub extern "C" fn answer() -> i32 {
    internal_answer()
}
