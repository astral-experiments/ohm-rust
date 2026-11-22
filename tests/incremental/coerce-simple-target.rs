//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]

pub struct Wrapper<T: ?Sized>(T);

// A generic struct can still unsize through its tail.
pub fn tail(x: &Wrapper<[u8; 4]>) -> &Wrapper<[u8]> {
    x
}

// Ordinary deref coercions and mutable reborrows must still happen.
pub fn slices(x: &mut [u8], v: &Vec<u8>, s: &String) {
    fn take(_: &mut [u8], _: &[u8], _: &str) {}
    take(x, v, s);
    take(x, v, s);
}

pub enum Item { Value }

#[cfg(bpass1)]
type Output<'a> = Option<&'a Item>;
#[cfg(bfail2)]
type Output<'a> = Option<&'static Item>;

// Ruling out unsizing must not bypass subtyping of the enum arguments.
pub fn borrow<'a>(x: &'a mut Item) -> Output<'a> {
    let y: &Item = x;
    let v = Some(y);
    v
    //[bfail2]~^ ERROR lifetime may not live long enough
}
