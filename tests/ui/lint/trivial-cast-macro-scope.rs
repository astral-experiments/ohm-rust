//@ revisions: normal internal
//@[internal] compile-flags: -Zunstable-options
//@ aux-build: trivial-cast-macros.rs
//@ check-pass

#![deny(trivial_casts, trivial_numeric_casts, unfulfilled_lint_expectations)]

extern crate trivial_cast_macros;
use trivial_cast_macros::find_attr;

fn main() {
    let number = 1u32;
    let _ = trivial_cast_macros::external_cast!(&number);

    #[cfg_attr(internal, expect(trivial_numeric_casts))]
    let _ = find_attr!(number);

    #[expect(trivial_numeric_casts)]
    let _ = number as u32;

    #[expect(trivial_casts)]
    let _ = &number as *const u32;
}
