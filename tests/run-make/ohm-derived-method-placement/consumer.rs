extern crate compare;
use compare::{Large, callers, first, second};

callers!(left);
callers!(right);

fn main() {
    assert!(std::ptr::fn_addr_eq(first::compare().0, second::compare().0));
    assert!(std::ptr::fn_addr_eq(first::copy(), second::copy()));

    let a = Large::A(&3, "alpha".into(), "beta".into(), "gamma".into());
    let b = Large::A(&4, "alpha".into(), "beta".into(), "gamma".into());
    let z = Large::Z(&3, "alpha".into(), "beta".into(), "gamma".into());
    for eq in [first::compare().0, second::compare().0, left::compare().0, right::compare().0] {
        assert!(eq(&a, &a));
        assert!(!eq(&a, &b));
        assert!(!eq(&a, &z));
    }
    for copy in [first::copy(), second::copy(), left::copy(), right::copy()] {
        assert!(first::compare().0(&a, &copy(&a)));
        assert!(first::compare().0(&z, &copy(&z)));
    }
}
