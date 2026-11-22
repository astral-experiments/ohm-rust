//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]

pub struct Tag;
pub trait Project<'a> {
    type Output;
}

impl<'a> Project<'a> for Tag {
    #[cfg(bpass1)]
    type Output = &'a u8;
    #[cfg(bfail2)]
    type Output = u8;
}

// The normalized projection contributes an implied bound only in the
// first revision. The referent in the ordinary reference is always u8.
pub fn project<'a, 'b>(_: &'b <Tag as Project<'a>>::Output, x: &'a u8) -> &'b u8 {
    x
    //[bfail2]~^ ERROR lifetime may not live long enough
}

// A generic referent still contributes an outlives bound.
pub fn required<'a, T: 'a>() {}
pub fn generic<'a, T>(_: &'a T) {
    required::<'a, T>();
}
