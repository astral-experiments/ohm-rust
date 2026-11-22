//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]
#![feature(trivial_bounds)]

pub trait Project {
    type Output;
}

// Adding an inferred outlives bound makes the projection ill-formed.
#[cfg(bpass1)]
pub struct Pair<'a, 'b>(pub &'a u8, pub &'b u8);
#[cfg(bfail2)]
pub struct Pair<'a, 'b>(pub &'a &'b u8);

impl<'a, 'b> Project for (&'a u8, &'b u8) {
    type Output = Pair<'a, 'b>;
    //[bfail2]~^ ERROR lifetime bound not satisfied
}

pub trait Marker {}
#[cfg(bpass1)]
pub struct Unit;
#[cfg(bfail2)]
pub struct Unit where (): Marker;

// An empty argument list does not imply an empty list of clauses.
pub fn accept(_: &Unit) {}
//[bfail2]~^ ERROR the trait bound `(): Marker` is not satisfied
