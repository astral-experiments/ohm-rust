//@ revisions: current next
//@[next] compile-flags: -Znext-solver

struct Bytes<'a, const N: usize>(&'a [u8; N]);

fn distinct_arguments<'a>(
    permanent: Bytes<'static, 4>,
    temporary: Bytes<'a, 4>,
) -> &'static [u8; 4] {
    let _: &'static [u8; 4] = permanent.0;
    // Reusing the composite field template must retain the second argument's lifetime.
    temporary.0
    //~^ ERROR lifetime may not live long enough
}

fn main() {}
