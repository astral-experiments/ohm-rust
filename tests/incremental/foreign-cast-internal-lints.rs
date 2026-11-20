//@ revisions: bpass1 bfail2
//@ aux-build: foreign-cast-find-attr.rs
//@[bfail2] compile-flags: -Zunstable-options -Zassert-incr-state=loaded

#![crate_type = "rlib"]

extern crate foreign_cast_find_attr;
use foreign_cast_find_attr::find_attr;

pub fn trigger(value: u32) -> u32 {
    #[cfg_attr(bpass1, allow(trivial_numeric_casts))]
    #[cfg_attr(bfail2, deny(trivial_numeric_casts))]
    let cast = find_attr!(value);
    //[bfail2]~^ ERROR trivial numeric cast
    cast
}
