//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

trait Project { type Value: Clone; }
impl<T: Clone> Project for Vec<T> { type Value = T; }

trait Sequences {
    fn identity<T>(value: T) -> T;
    fn projected<T: Project>(value: T::Value) -> impl Iterator<Item = T::Value> + Clone;
    fn borrowed<'a, T: 'a>(values: &'a [T]) -> impl Iterator<Item = &'a T> + Clone + 'a;
}

impl Sequences for () {
    fn identity<T>(value: T) -> T { value }

    // Normalizing the generic input and relating the opaque output introduce
    // obligations at different points in the impl/trait comparison.
    fn projected<T: Project>(value: T::Value) -> impl Iterator<Item = T::Value> + Clone {
        std::iter::once(value).cycle().take(2)
    }

    fn borrowed<'a, T: 'a>(values: &'a [T]) -> impl Iterator<Item = &'a T> + Clone + 'a {
        values.iter().map(|value| value)
    }
}

fn compose<T: Project>(value: T::Value) -> impl Iterator<Item = T::Value> + Clone {
    <() as Sequences>::projected::<T>(value).map(<() as Sequences>::identity)
}

fn main() {
    let values = compose::<Vec<String>>(String::from("repeated"));
    assert_eq!(values.clone().collect::<Vec<_>>(), ["repeated", "repeated"]);
    assert_eq!(values.collect::<Vec<_>>(), ["repeated", "repeated"]);

    let values = [String::from("first"), String::from("second")];
    let borrowed = <() as Sequences>::borrowed(&values);
    assert_eq!(borrowed.clone().next(), Some(&values[0]));
    assert_eq!(borrowed.last(), Some(&values[1]));
}
