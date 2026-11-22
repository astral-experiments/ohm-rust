//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]

pub struct Contains<'a>(pub &'a u8);

// Calling this function requires the nested input lifetime to outlive
// the outer one. An inferred region, as well as a concrete reference
// to a nominal type, must retain that outlives requirement.
pub fn shorten<'x, 'y>(_: &'x Contains<'y>, x: &'y u8) -> &'x u8 {
    x
}

pub fn call<'a, 'b>(witness: &'a Contains<'static>, x: &'b u8) -> &'a u8 {
    #[cfg(bpass1)]
    { shorten(witness, &0) }
    #[cfg(bfail2)]
    { shorten(witness, x) }
    //[bfail2]~^ ERROR lifetime may not live long enough
}
