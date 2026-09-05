//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths

trait StaticOnly {}
impl StaticOnly for &'static u32 {}

fn require<T: StaticOnly>(_: T) {}

fn main() {
    let value = 7;
    let borrowed = &value;
    //~^ ERROR `value` does not live long enough
    // Repeated non-Copy trait proofs must retain the impl's region requirements.
    require(borrowed);
    require(borrowed);
    require(borrowed);
}
