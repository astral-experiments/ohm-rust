//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]

#[derive(Clone)]
pub struct Pair<'a, 'b>(pub &'a u32, pub &'b u32);

#[cfg(bpass1)]
impl<'a, 'b> Copy for Pair<'a, 'b> {}
#[cfg(bfail2)]
impl<'a: 'b, 'b> Copy for Pair<'a, 'b> {}

// MIR still has to prove the Copy operands and preserve their constraints
// while normalizing and checking the sizedness of the surrounding assignments.
pub fn duplicate<'a, 'b>(x: Pair<'a, 'b>) -> (Pair<'a, 'b>, Pair<'a, 'b>) {
    let y = x;
    //[bfail2]~^ ERROR lifetime may not live long enough
    (x, y)
}
