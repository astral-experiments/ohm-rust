#![cfg_attr(expect_lint, expect(unused_crate_dependencies))]

#[cfg(use_dep)]
use dep as _;

fn main() {
    #[cfg(use_bridge)]
    assert_eq!(bridge::value(), 42);
}
