//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

trait Method {
    fn identity<T: Clone>(value: T) -> T;
}

impl Method for () {
    // Matching signature types must not hide an unsatisfied impl bound.
    fn identity<T: Clone + Copy>(value: T) -> T { value }
    //~^ ERROR impl has stricter requirements than trait
}

trait Sequence {
    fn values<T: Clone>(value: T) -> impl Iterator<Item = T> + Clone;
}

impl Sequence for () {
    fn values<T: Clone>(_: T) -> impl Iterator<Item = T> + Clone {
        //~^ ERROR expected `Once<()>` to be an iterator that yields `T`, but it yields `()`
        std::iter::once(())
    }
}

trait Project { type Value; }
impl<T> Project for Vec<T> { type Value = T; }

struct Watch<'a>(&'a String);
impl Drop for Watch<'_> {
    fn drop(&mut self) { assert!(!self.0.is_empty()); }
}

struct ProjectedPair<T: Project> {
    first: T::Value,
    second: Option<T::Value>,
}

fn projected_drop() {
    let _pair;
    {
        let first = String::from("first");
        let second = String::from("second");
        _pair = ProjectedPair::<Vec<_>> {
            first: Watch(&first),
            //~^ ERROR `first` does not live long enough
            second: Some(Watch(&second)),
            //~^ ERROR `second` does not live long enough
        };
    }
}

fn main() {}
