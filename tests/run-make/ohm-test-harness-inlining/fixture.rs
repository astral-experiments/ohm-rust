#[inline(never)]
#[no_mangle]
pub extern "C" fn step(value: u32) -> u32 {
    unsafe { std::ptr::read_volatile(&value) }
}

#[inline]
pub fn hinted(value: u32) -> u32 {
    let a = step(value);
    let b = step(a);
    let c = step(b);
    let d = step(c);
    let e = step(d);
    step(e)
}

#[used]
static KEEP: fn(u32) -> u32 = hinted;

#[no_mangle]
pub fn caller(value: u32) -> u32 {
    hinted(value)
}

#[test]
fn calls_hint() {
    assert_eq!(caller(42), 42);
}
