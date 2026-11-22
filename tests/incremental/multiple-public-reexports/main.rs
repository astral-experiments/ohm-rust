//@ revisions: rpass1 rpass2 rpass3
//@ aux-build: defs.rs
//@ compile-flags: -Zincremental-verify-ich

extern crate defs;

fn main() {
    use defs::other::Factory;
    let product: <defs::other::Maker as defs::Factory>::Product =
        defs::Maker::create().next().unwrap();
    assert_eq!(std::mem::size_of_val(&product), if cfg!(rpass1) { 1 } else { 2 });
    assert_eq!(product, 7);
    #[cfg(rpass2)]
    assert_eq!(defs::api::Maker::create().next(), Some(product));
}
