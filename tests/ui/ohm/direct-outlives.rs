//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![feature(adt_const_params)]

#[derive(Clone, Copy)]
struct Borrowed<'a>(&'a str);
struct ConstArray<const BYTES: [u8; 4]>;
struct LargeConstArray<const BYTES: [u8; 80]>;

fn require<'a, T: 'a>(value: T) -> T { value }
fn require_region<'a, 'b: 'a>(_: &'a (), _: &'b ()) {}

fn generic<'a, T: 'a>(value: T) -> T { require::<'a, T>(value) }

fn unequal_regions<'a: 'b, 'b>(long: &'a (), short: &'b ()) {
    require_region::<'b, 'a>(short, long);
}

fn nested_generic<'a: 'b, 'b, T: 'a>(
    value: (&'a T, [Option<T>; 1]),
    marker: &'b (),
) -> (&'a T, [Option<T>; 1]) {
    // Preserve both T: 'b and 'a: 'b through successive operations.
    require_region::<'b, 'a>(marker, &());
    let value = require::<'b, _>(value);
    require::<'b, _>(value)
}
fn generic_reference<'a, T>(value: &'a T) -> &'a T { require::<'a, &'a T>(value) }
fn nested_reference<'a>(value: &'a &'a str) -> &'a &'a str { require::<'a, _>(value) }
fn generic_const<const N: usize>(value: [u8; N]) -> [u8; N] {
    require::<'static, [u8; N]>(value)
}

trait Project { type Output; }
impl Project for () { type Output = String; }
fn projection<'a, T: Project>(value: T::Output) -> T::Output
where T::Output: 'a {
    require::<'a, T::Output>(value)
}

fn check<'a>(text: &'a str, marker: &'a ()) {
    require_region::<'a, 'a>(marker, marker);
    require_region::<'a, 'static>(marker, &());
    assert_eq!(require::<'static, _>(String::from("owned")), "owned");
    assert_eq!(require::<'a, _>(Borrowed(text)).0, text);
    let tuple = require::<'a, _>((Borrowed(text), [1u8, 2, 3], "static"));
    assert_eq!(tuple.0.0, text);
    assert_eq!(tuple.1[1], 2);
    assert_eq!(require::<'a, _>(text), text);
    assert_eq!(generic::<'a, _>(Borrowed(text)).0, text);
    assert_eq!(generic_reference(&tuple.1), &[1, 2, 3]);
    unequal_regions(marker, &());
    let nested = nested_generic((&tuple.1, [Some([7, 8, 9])]), marker);
    assert_eq!(*nested.0, [1, 2, 3]);
    assert_eq!(nested.1[0], Some([7, 8, 9]));
    assert_eq!(nested_reference(&text), &text);
    assert_eq!(generic_const([4, 5]), [4, 5]);
    assert_eq!(projection::<'static, ()>(String::from("projected")), "projected");
    let closure = require::<'a, _>(move || text);
    assert_eq!(closure(), text);

    // Shared subtrees retain their outlives requirements.
    let pair = (text, text);
    let pair = (pair, pair);
    let pair = (pair, pair);
    let pair = (pair, pair);
    let pair = (pair, pair);
    let pair = (pair, pair);
    let pair = require::<'a, _>(pair);
    assert_eq!(pair.0.0.0.0.0.0, text);
}

fn consume(_: &str) {}

fn main() {
    // A nested bound variable keeps this type on the canonical fallback.
    let function: for<'a> fn(&'a str) = consume;
    require::<'static, _>(function)("higher-ranked");

    // Preserve aggregate const values and their typed children.
    require::<'static, _>(ConstArray::<{ [1, 2, 3, 4] }>);
    require::<'static, _>(LargeConstArray::<{ [0; 80] }>);
    let text = String::from("borrowed");
    check(&text, &());
}
