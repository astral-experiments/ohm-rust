//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

use std::cell::Cell;
use std::ops::Deref;

struct Wrap<T>(T);

impl<T> Deref for Wrap<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}

trait Summarize {
    fn summarize(&self) -> u32;
}

impl Summarize for [u32] {
    fn summarize(&self) -> u32 { self.iter().copied().sum() }
}

fn apply<F>(function: F, value: &u32) -> &u32
where F: for<'a> Fn(&'a u32) -> &'a u32,
{
    function(value)
}

fn identity<T>(value: T) -> T { value }
fn borrow<T>(value: &T) -> &T { value }

trait Project { type Value; }
impl<T> Project for Vec<T> { type Value = T; }

struct ProjectedPair<T: Project> {
    first: T::Value,
    second: Option<T::Value>,
}

struct CountDrop<'a>(&'a Cell<u32>);
impl Drop for CountDrop<'_> {
    fn drop(&mut self) { self.0.set(self.0.get() + 1); }
}

fn drop_projected<T: Project>(value: ProjectedPair<T>) {
    // The repeated projection must preserve every destructor's lifetime needs.
    let _ = (&value.first, &value.second);
    drop(value);
}

fn main() {
    // Plain type relations precede method probes with autoderef and unsizing.
    let wrapped = identity(Wrap(Wrap([2, 3, 5])));
    assert_eq!(wrapped.summarize(), 10);
    let array: &[u32; 3] = &wrapped;
    let slice: &[u32] = array;
    assert_eq!(slice, &[2, 3, 5]);

    // Higher-ranked relations occur inside nested function and closure probes.
    let value = 13;
    assert_eq!(*apply(borrow, &value), 13);
    let function: &dyn for<'a> Fn(&'a u32) -> &'a u32 = &borrow;
    assert_eq!(*apply(|value| apply(function, value), &value), 13);

    let dropped = Cell::new(0);
    drop_projected::<Vec<_>>(ProjectedPair {
        first: CountDrop(&dropped),
        second: Some(CountDrop(&dropped)),
    });
    assert_eq!(dropped.get(), 2);
}
