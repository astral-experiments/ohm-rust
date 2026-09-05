//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

fn require_forall<T>(_: T) where for<'a> T: 'a {}
fn require_static<T: 'static>(_: T) {}

fn higher_ranked<'a>(value: &'a str) {
    require_forall(value);
    //~^ ERROR borrowed data escapes outside of function
}

fn captured<'a>(value: &'a str) {
    require_static(move || value);
    //~^ ERROR borrowed data escapes outside of function
}

fn higher_ranked_type<'a>(value: for<'b> fn(&'b &'a str)) {
    require_static::<for<'b> fn(&'b &'a str)>(value);
    //~^ ERROR lifetime may not live long enough
}

fn main() {
    require_forall(String::from("owned"));
    require_forall("static");
}
