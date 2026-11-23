//@ revisions: rpass1 rpass2 rpass3
//@ compile-flags: -Zpublic-api-hash -Zincremental-verify-ich
//@ aux-build: dep.rs

extern crate dep;
use dep::api::*;

// The body stays the same when the upstream reexport changes target.
// Both the type and the constructor must resolve to the new definition.
fn value() -> u32 {
    let item: Selected = Selected;
    item.number() + OFFSET
}

fn main() {
    assert_eq!(value(), if cfg!(rpass2) { 12 } else { 11 });
}
