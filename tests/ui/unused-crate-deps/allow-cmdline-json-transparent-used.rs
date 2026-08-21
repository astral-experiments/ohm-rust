// An empty report distinguishes a successful scan from a compiler that does not report unused
// externs while the lint is allowed.

//@ edition:2018
//@ check-pass
//@ compile-flags: --json unused-externs-transparent --error-format=json
//@ aux-crate:bar=bar.rs

fn main() {
    assert_eq!(bar::BAR, "bar");
}
