//! Type-check named function parameters directly against their known signature types.

//@ check-pass

#![allow(dead_code)]

struct Handler<T>(T);

impl<T: Clone> Handler<T> {
    fn clone_argument(&self, item: T) -> T {
        item.clone()
    }

    fn increment(&self, mut value: usize) -> usize {
        value += 1;
        value
    }

    fn destructure(&self, (first, second): (T, T)) -> (T, T) {
        (second, first)
    }

    fn ref_binding(&self, ref item: T) -> T {
        item.clone()
    }
}

fn borrow<'a, T>(item: &'a T) -> &'a T {
    item
}

fn closure_parameter() {
    let identity = |item| item;
    let _: u32 = identity(5_u32);
}

fn main() {}
