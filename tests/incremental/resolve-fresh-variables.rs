//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]

pub trait Select<Other> {}

impl Select<f32> for (u8, u8) {}
impl Select<f64> for (u8, u32) {}

pub fn check<A, B, C>(_: A, _: B, _: C)
where
    (A, B): Select<C>,
{
}

// Repeated occurrences of an unknown must resolve to the same type,
// even when other unknown types and numeric variables occur in the goal.
pub fn test() {
    let a = 1;
    let b = 2;
    let f = 0.0;
    check(a, a, f);
    check(a, b, 0.0);
    let _: u8 = a;
    let _: u32 = b;

    #[cfg(bpass1)]
    let _: f32 = f;
    #[cfg(bfail2)]
    let _: f64 = f;
    //[bfail2]~^ ERROR mismatched types
}
