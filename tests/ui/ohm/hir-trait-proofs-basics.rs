//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Token<'a>(&'a str);
trait Ordinary {}
impl Ordinary for Token<'_> {}

const fn require<T: Ordinary + Eq + Hash + Copy>() {}
fn require_copy<T: Copy>(_: &Option<T>) {}

impl<'a> Token<'a> {
    fn checked(&self) -> Self {
        require::<Token<'a>>();
        *self
    }
}

fn check<'a>(value: Token<'a>) {
    require::<Token<'a>>();
    let copied = value;
    assert_eq!(copied.checked(), value);

    let closure = || {
        require::<Token<'a>>();
        value
    };
    assert_eq!(closure(), value);
    const { require::<Token<'a>>(); }

    // The Copy goal starts with an unresolved type, then becomes eligible.
    let mut delayed = None;
    require_copy(&delayed);
    assert!(delayed.is_none());
    delayed = Some(value);
    let copied = delayed;
    assert_eq!(copied, delayed);

    let mut left = DefaultHasher::new();
    let mut right = DefaultHasher::new();
    value.hash(&mut left);
    copied.unwrap().hash(&mut right);
    assert_eq!(left.finish(), right.finish());
}

fn main() {
    let text = String::from("borrowed");
    check(Token(&text));
}
