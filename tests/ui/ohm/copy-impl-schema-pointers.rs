//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#[derive(Debug)]
struct NotCopy;

fn duplicate_shared<T: ?Sized>(value: &T) -> (&T, &T) {
    (value, value)
}

fn duplicate_const<T: ?Sized>(value: *const T) -> (*const T, *const T) {
    (value, value)
}

fn duplicate_mut<T: ?Sized>(value: *mut T) -> (*mut T, *mut T) {
    (value, value)
}

fn with_copy_bound<'a, T: ?Sized + 'a>(value: &'a T) -> (&'a T, &'a T)
where
    &'a T: Copy,
{
    (value, value)
}

fn check<T: ?Sized>(value: &mut T) {
    let (left, right) = duplicate_shared(&*value);
    assert!(std::ptr::eq(left, right));
    let (left, right) = duplicate_const(std::ptr::from_ref(&*value));
    assert!(std::ptr::eq(left, right));
    let (left, right) = duplicate_mut(std::ptr::from_mut(value));
    assert!(std::ptr::eq(left, right));
}

fn main() {
    check(&mut 42_u8);
    let mut non_copy = NotCopy;
    check(&mut non_copy);
    check(&mut [NotCopy, NotCopy][..]);
    check(String::from("unsized").as_mut_str());
    check(&mut non_copy as &mut dyn std::fmt::Debug);
    let (left, right) = with_copy_bound(&non_copy);
    assert!(std::ptr::eq(left, right));
}
