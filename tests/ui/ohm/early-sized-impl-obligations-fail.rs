//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths

trait Project {
    type Output: ?Sized;
}

struct Unsized;
impl Project for Unsized {
    type Output = str;
}

trait Check {}
// The implicit Sized predicates precede this failing projection bound. Its
// diagnostic must still identify the original predicate index and source span.
impl<T: Project, U> Check for (T, U) where T::Output: Sized {}

fn require<T: Check>(_: T) {}

fn main() {
    require((Unsized, 1u8));
    //~^ ERROR the size for values of type `str` cannot be known at compilation time
}
