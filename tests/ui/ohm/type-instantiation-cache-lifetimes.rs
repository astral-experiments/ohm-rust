//@ revisions: current next
//@[next] compile-flags: -Znext-solver

struct Wrapped<'a>(&'a [u8; 4]);

fn different_arguments<'a>(permanent: Wrapped<'static>, temporary: Wrapped<'a>) -> &'static [u8; 4] {
    let _: &'static [u8; 4] = permanent.0;
    // Reusing this field template must retain the second argument's lifetime.
    temporary.0
    //~^ ERROR lifetime may not live long enough
}

fn main() {}
