// Loud unused-extern reporting preserves the existing default-allow behavior.

//@ edition:2018
//@ check-pass
//@ compile-flags: --json unused-externs --error-format=json
//@ aux-crate:bar=bar.rs

fn main() {}
