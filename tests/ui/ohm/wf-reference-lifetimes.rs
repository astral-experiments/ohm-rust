//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

fn require<T>() {}

fn nested<'a, 'b>() {
    require::<&'a &'b str>();
    //~^ ERROR lifetime may not live long enough
}

fn generic<'a, T>() {
    require::<&'a T>();
    //~^ ERROR the parameter type `T` may not live long enough
}

fn nominal<'a, T>() {
    require::<Option<&'a T>>();
    //~^ ERROR the parameter type `T` may not live long enough
}

fn pointer<'a, T>() {
    require::<*const &'a T>();
    //~^ ERROR the parameter type `T` may not live long enough
}

fn repeated<'a, T>() {
    require::<Result<&'a T, &'a T>>();
    //~^ ERROR the parameter type `T` may not live long enough
}

fn escape<'a>(target: &mut Option<&'a String>) {
    let value = String::from("local");
    *target = Some(&value);
    //~^ ERROR `value` does not live long enough
}

fn main() {}
