//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver=globally
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

struct Probe<T>(T);

impl Probe<(u8, bool)> {
    fn choose(&self) -> u8 {
        1
    }
}

impl Probe<(u16, i32)> {
    fn choose(&self) -> u8 {
        2
    }
}

fn identity<T>(value: T) -> T {
    value
}
fn same<T>(_: &T, _: &T) {}

fn main() {
    let probe = Probe((0, 0));
    assert_eq!(probe.choose(), 2);
    assert_eq!(Probe((0u8, false)).choose(), 1);
    let mut values = identity(Vec::new());
    values.push([3u16, 5]);
    for _ in 0..4 {
        same(&values, &vec![[7u16, 11]]);
        assert_eq!(values[0], [3, 5]);
    }
    let floats = identity([1.25, 2.5]);
    same(&floats, &[0.0f64, 0.0]);
    assert_eq!(floats.iter().sum::<f64>(), 3.75);
}
