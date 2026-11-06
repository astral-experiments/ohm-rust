#![feature(rustc_attrs)]
#![allow(internal_features)]

const VALUE: u32 = 1;
struct Anchor;
#[derive(observe::Observe)]
struct Subject;

fn main() {
    assert_ne!(VALUE, 0);
    assert_eq!(Subject::unchanged("ok"), 2);
    println!("{}", Subject::location());
}

struct LaterAnchor;
