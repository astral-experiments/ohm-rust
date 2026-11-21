extern crate compare;
use compare::{EqFn, Large, first, second};

fn main() {
    assert!(std::ptr::fn_addr_eq(first::compare().0, second::compare().0));
    let downstream: EqFn<Large<'static>> = Large::eq;
    for eq in [first::compare().0, second::compare().0, downstream] {
        assert!(eq(&Large::A(&3), &Large::A(&3)));
        assert!(!eq(&Large::A(&3), &Large::A(&4)));
        assert!(!eq(&Large::A(&3), &Large::Z(&3)));
    }
}
