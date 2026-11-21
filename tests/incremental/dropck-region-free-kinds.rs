//@ revisions: bpass1 bpass2 bfail3
//@ compile-flags: -Zquery-dep-graph -Zincremental-ignore-spans -Zthreads=1

#![crate_type = "lib"]
#![feature(rustc_attrs)]
#![allow(internal_features, dead_code)]

struct Own<T>(T);

impl<T> Drop for Own<T> {
    fn drop(&mut self) {}
}

struct Bag<'a> {
    marker: &'a str,
    old: Own<u8>,
    #[cfg(bpass2)]
    owned: Own<String>,
    #[cfg(bfail3)]
    owned: Own<&'a str>,
}

// Keep the field list of the type returned by `make` unchanged.
struct Outer<'a>(Bag<'a>);

fn make(text: &str) -> Outer<'_> {
    Outer(Bag {
        marker: text,
        old: Own(0),
        #[cfg(bpass2)]
        owned: Own(String::new()),
        #[cfg(bfail3)]
        owned: Own(text),
    })
}

pub fn first<T>() {
    // Recompute the drop-check goal in the same parameter environment as
    // `second`. Only the second revision needs MIR for its instantiation.
    let text = String::new();
    let _bag = make(&text);
    #[cfg(bpass2)]
    second::<T>();
}

// Adding a field with an owned String should leave the drop-check result
// unchanged. Replacing it with a borrow must make `text` live until drop.
#[rustc_clean(cfg = "bpass2", loaded_from_disk = "mir_borrowck")]
fn second<T>() {
    let text = String::new();
    let _bag = make(&text);
    drop(text);
    //[bfail3]~^ ERROR cannot move out of `text` because it is borrowed
}
