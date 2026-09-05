//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![allow(dead_code)]

#[derive(Clone)]
struct BadField(String);
impl Copy for BadField {}
//~^ ERROR the trait `Copy` cannot be implemented for this type

#[derive(Clone)]
struct HasDrop;
impl Copy for HasDrop {}
//~^ ERROR the trait `Copy` cannot be implemented for this type; the type has a destructor
impl Drop for HasDrop {
    fn drop(&mut self) {}
}

fn copy_bad(value: &BadField) -> BadField {
    *value
}

fn copy_drop(value: &HasDrop) -> HasDrop {
    *value
}

fn main() {}
