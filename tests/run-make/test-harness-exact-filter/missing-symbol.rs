unsafe extern "C" {
    fn missing_only_from_unselected_test();
}

#[test]
fn selected() {}

#[test]
fn unselected() {
    unsafe { missing_only_from_unselected_test() };
}
