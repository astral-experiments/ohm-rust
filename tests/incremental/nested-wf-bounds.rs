//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]

pub trait Marker {}

#[cfg(bpass1)]
pub struct Payload<T>(T);
#[cfg(bfail2)]
pub struct Payload<T: Marker>(T);

pub fn accept(_: Option<&Payload<[u8; 8]>>) {}
//[bfail2]~^ ERROR the trait bound `[u8; 8]: Marker` is not satisfied

pub fn project<'a, 'b>(x: &'a &'b u8) -> &'a u8 {
    *x
}

pub fn require_nested<'a, 'b>(x: &'b u8) -> &'a u8 {
    #[cfg(bpass1)]
    { &0 }
    #[cfg(bfail2)]
    { project(&x) }
    //[bfail2]~^ ERROR lifetime may not live long enough
    //[bfail2]~| ERROR cannot return value referencing function parameter `x`
}
