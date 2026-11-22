//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]

#[cfg(bpass1)]
type Element = u8;
#[cfg(bfail2)]
type Element = str;

// An unsized tuple tail is permitted. Every preceding field and every
// array element must still be sized, even when the array is empty.
pub fn tail(_: &((), Element)) {}

pub fn head(_: &(Element, ())) {}
//[bfail2]~^ ERROR the size for values of type `str` cannot be known at compilation time

pub fn array(_: &[Element; 0]) {}
//[bfail2]~^ ERROR the size for values of type `str` cannot be known at compilation time

pub struct Wrapper<T: ?Sized>(pub T);
pub fn unsized_arg(_: &Wrapper<Element>) {}
