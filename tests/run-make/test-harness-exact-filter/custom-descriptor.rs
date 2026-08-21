#![feature(custom_test_frameworks, test)]

extern crate test;

#[test_case]
const CASE: test::TestDescAndFn = test::TestDescAndFn::new_doctest(
    "actual",
    false,
    "custom-descriptor.rs",
    1,
    false,
    false,
    test::StaticTestFn(run),
);

fn run() -> Result<(), String> {
    Ok(())
}
