//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

struct Pair<A, B>(A, B);

fn generic<T, const N: usize>(value: T, array: [u8; N]) -> Option<Pair<T, [u8; N]>> {
    let result;
    result = Some(Pair(value, array));
    result
}

fn nested<T>(value: T) -> Pair<(T, ()), Option<*const u8>> {
    let result;
    result = Pair((value, ()), Some(std::ptr::null()));
    result
}

trait Project {
    type Output;
}
impl Project for () {
    type Output = u32;
}
fn projection<T: Project>(value: T::Output) -> Option<T::Output> {
    let result;
    result = Some(value);
    result
}

fn opaque() -> impl Iterator<Item = u32> {
    [1, 2, 3].into_iter()
}

fn reference<'a>(value: &'a u32) -> Option<&'a u32> {
    let result;
    result = Some(value);
    result
}

fn mutable<'a>(value: &'a mut u32) -> Option<&'a mut u32> {
    let result;
    result = Some(value);
    result
}

fn identity(value: &u32) -> &u32 { value }
fn higher_ranked(value: for<'a> fn(&'a u32) -> &'a u32) -> u32 {
    let result;
    result = Some(value);
    *result.unwrap()(&42)
}

fn main() {
    let Pair(text, array) = generic(String::from("owned"), [1, 2, 3]).unwrap();
    assert_eq!(text, "owned");
    assert_eq!(array, [1, 2, 3]);
    let Pair((value, ()), pointer) = nested(vec![vec![1_u32, 2]]);
    assert_eq!(value, vec![vec![1, 2]]);
    assert!(pointer.unwrap().is_null());
    assert_eq!(projection::<()>(42), Some(42));
    let iterator;
    iterator = Some(opaque());
    assert_eq!(iterator.unwrap().sum::<u32>(), 6);
    let mut value = 7;
    assert_eq!(*reference(&value).unwrap(), 7);
    *mutable(&mut value).unwrap() = 8;
    assert_eq!(value, 8);
    assert_eq!(higher_ranked(identity), 42);
}
