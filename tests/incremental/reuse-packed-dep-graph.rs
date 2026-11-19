//@ revisions: rpass1 rpass2 rpass3 rpass4

#![allow(dead_code)]

const FIRST: u32 = 3;
const SECOND: u32 = if cfg!(any(rpass3, rpass4)) { 5 } else { 3 };

// In rpass2 SELECTED still evaluates to 3. Its new edge to SECOND
// must survive when the unchanged parent reuses its previous edges.
#[cfg(any(rpass1, rpass4))]
const SELECTED: u32 = FIRST;
#[cfg(any(rpass2, rpass3))]
const SELECTED: u32 = SECOND;

const DOUBLED: u32 = SELECTED * 2;

fn main() {
    assert_eq!(DOUBLED, if cfg!(rpass3) { 10 } else { 6 });
}
