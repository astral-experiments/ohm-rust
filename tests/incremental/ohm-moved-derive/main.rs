//@ revisions: rpass1 rpass2
//@ aux-build: macros.rs
//@ no-prefer-dynamic
//@ compile-flags: -Zquery-dep-graph -Copt-level=1 -Cdebuginfo=line-tables-only
//@ ignore-backends: gcc

#![feature(rustc_attrs)]
#![allow(internal_features)]
// Moving the invocation preserves the definition-site locations in Key::hash.
#![rustc_partition_reused(module = "main-hashing", cfg = "rpass2")]

extern crate macros;

trait Line {
    fn line(&self) -> u32;
}

#[track_caller]
#[inline(never)]
fn tracked() -> u32 {
    std::panic::Location::caller().line()
}

#[cfg(rpass1)]
macros::make!(Key);
#[cfg(rpass2)]
macros::make!(Key);

mod hashing {
    use std::hash::{DefaultHasher, Hash, Hasher};

    #[inline(never)]
    pub fn hash() -> u64 {
        let mut hasher = DefaultHasher::new();
        super::Key(1).hash(&mut hasher);
        hasher.finish()
    }
}

fn main() {
    std::hint::black_box(hashing::hash());
    // The other automatically derived method must report the new invocation site.
    assert_eq!(Key(1).line(), SITE);
}
