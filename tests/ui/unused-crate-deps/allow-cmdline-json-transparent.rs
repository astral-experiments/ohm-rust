// Transparent unused-extern reports remain available when the lint is allowed.
// `bar` is unused as a direct extern even though `foo` loads it transitively.

//@ edition:2018
//@ check-pass
//@ compile-flags: --json unused-externs-transparent --error-format=json
//@ aux-crate:bar=bar.rs
//@ aux-crate:foo=foo.rs

fn main() {
    assert_eq!(foo::BAR, "bar");
}
