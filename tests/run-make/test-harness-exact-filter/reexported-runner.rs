#![feature(custom_test_frameworks)]
#![reexport_test_harness_main = "test_main"]

#[test]
fn selected() {}

#[test]
fn unselected() {}

fn call_reexported_runner() {
    test_main();
}
