//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

fn require<'a, T: 'a>(_: T) {}
fn require_region<'a, 'b: 'a>(_: &'a (), _: &'b ()) {}
struct Pair<'a, 'b>(&'a str, &'b str);

fn regions<'a, 'b>(a: &'a (), b: &'b ()) {
    require_region::<'a, 'b>(a, b);
    //~^ ERROR lifetime may not live long enough
}

fn borrowed<'a>(value: &'a str) {
    require::<'static, _>(value);
    //~^ ERROR borrowed data escapes outside of function
}

fn unequal<'a, 'b>(value: Pair<'a, 'b>) {
    require::<'a, _>(value);
    //~^ ERROR lifetime may not live long enough
}

fn generic<T>(value: T) {
    require::<'static, _>(value);
    //~^ ERROR the parameter type `T` may not live long enough
}

fn nested_generic<'a, T>(value: [Option<T>; 1]) {
    require::<'a, _>(value);
    //~^ ERROR the parameter type `T` may not live long enough
}

trait Project { type Output; }
fn projection<T: Project>(value: T::Output) {
    require::<'static, _>(value);
    //~^ ERROR the associated type `<T as Project>::Output` may not live long enough
}

fn generic_const<'a, const N: usize>(value: [&'a str; N]) {
    require::<'static, _>(value);
    //~^ ERROR borrowed data escapes outside of function
}

fn main() {}
