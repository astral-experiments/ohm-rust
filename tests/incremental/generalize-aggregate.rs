//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]

pub fn array<T, const N: usize>(x: [T; N]) -> Option<[T; N]> {
    let mut y = None;
    y.replace(x);
    y
}

pub struct Tag;
pub trait Project<'a> {
    type Item;
}

impl<'a> Project<'a> for Tag {
    #[cfg(bpass1)]
    type Item = [u8; 4];
    #[cfg(bfail2)]
    type Item = &'a [u8; 4];
}

pub fn collect<'a>(x: <Tag as Project<'a>>::Item) -> Vec<<Tag as Project<'static>>::Item> {
    let mut v = vec![];
    v.push(x);
    v
    //[bfail2]~^ ERROR lifetime may not live long enough
}
