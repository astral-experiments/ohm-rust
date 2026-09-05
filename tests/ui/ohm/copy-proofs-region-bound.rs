//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths

#[derive(Clone)]
struct OnlyStatic<'a>(&'a u32);
impl Copy for OnlyStatic<'static> {}

fn consume<T>(_: T) {}

fn main() {
    let value = 7;
    let data = OnlyStatic(&value);
    //~^ ERROR `value` does not live long enough
    // Repeated Copy operations must retain the impl's region requirements.
    consume(data);
    consume(data);
    consume(data);
}
