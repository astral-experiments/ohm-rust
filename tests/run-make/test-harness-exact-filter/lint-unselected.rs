#![deny(unused_variables)]

#[test]
fn selected() {}

#[test]
fn unselected() {
    let must_still_lint = 1;
}
