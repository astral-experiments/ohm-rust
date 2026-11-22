//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]

pub trait Tr<'b> {}
pub struct Holder<'a>(&'a u8);

#[cfg(bpass1)]
impl<'a, 'b> Tr<'b> for Holder<'a> {}
#[cfg(bfail2)]
impl<'a: 'b, 'b> Tr<'b> for Holder<'a> {}

fn require<'b, T: Tr<'b>>(_: T) {}

pub fn check<'a, 'b>(x: Holder<'a>) {
    require::<'b, _>(x);
    //[bfail2]~^ ERROR lifetime may not live long enough
}
