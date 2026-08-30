//@ revisions: current next
//@[next] compile-flags: -Znext-solver
//@ ignore-compare-mode-next-solver (explicit revisions)
//@ run-pass

// Exercise the parent and own predicates of projections, method function items, and closures.
// Lifetimes and const parameters must retain their original substitution indices.
trait Family<'parent, T: Copy + 'parent, const N: usize> {
    type Item<'item, U: Clone + 'item>
    where
        Self: 'item,
        T: 'item;

    fn item<'item, U: Clone + 'item>(&'item self, value: U) -> Self::Item<'item, U>
    where
        T: 'item;
}

struct Holder<'parent, T: Copy, const N: usize>(&'parent T);

impl<'parent, T: Copy, const N: usize> Family<'parent, T, N> for Holder<'parent, T, N> {
    type Item<'item, U: Clone + 'item>
        = (T, U, [u8; N])
    where
        Self: 'item,
        T: 'item;

    fn item<'item, U: Clone + 'item>(&'item self, value: U) -> Self::Item<'item, U>
    where
        T: 'item,
    {
        (*self.0, value.clone(), [0; N])
    }
}

fn projected<'parent, T: Copy, const N: usize>(holder: &Holder<'parent, T, N>) -> (T, u8, [u8; N]) {
    let method = <Holder<'parent, T, N> as Family<'parent, T, N>>::item::<u8>;
    let closure = || method(holder, 7);
    let item: <Holder<'parent, T, N> as Family<'parent, T, N>>::Item<'_, u8> = closure();
    item
}

// Bound variables under a function-pointer binder must not escape into nominal obligations.
struct HigherRanked<F: for<'a> Fn(&'a u32) -> &'a u32>(F);

fn identity<T: Copy>(value: &T) -> &T {
    value
}

fn main() {
    let value = 5;
    assert_eq!(projected(&Holder::<_, 3>(&value)), (5, 7, [0; 3]));
    let higher_ranked = HigherRanked(identity::<u32> as for<'a> fn(&'a u32) -> &'a u32);
    assert_eq!((higher_ranked.0)(&value), &value);
}
