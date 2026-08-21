#![feature(custom_test_frameworks)]
#![test_runner(run_tests)]

#[test_case]
fn selected() {}

#[test_case]
fn unselected() {}

fn run_tests(tests: &[&dyn Fn()]) {
    assert_eq!(tests.len(), 2);
    println!("{} custom tests", tests.len());
    for test in tests {
        test();
    }
}
