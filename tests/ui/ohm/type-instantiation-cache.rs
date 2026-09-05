//@ run-pass
//@ revisions: current next
//@[next] compile-flags: -Znext-solver

trait Project {
    type Item: Copy;
}

impl Project for u8 { type Item = u16; }
impl Project for u16 { type Item = u32; }

struct Packet<'a, T: Project, const N: usize> {
    values: [T::Item; N],
    borrowed: &'a T::Item,
    select: for<'b> fn(&'a T::Item, &'b T::Item) -> &'b T::Item,
    input: T,
    plain: usize,
}

fn second<'a, 'b, T>(_: &'a T, value: &'b T) -> &'b T { value }

fn packet<T: Project, const N: usize>(
    borrowed: &T::Item,
    input: T,
    values: [T::Item; N],
) -> Packet<'_, T, N> {
    Packet { values, borrowed, select: second, input, plain: N }
}

fn main() {
    // The same field templates retain different type/const arguments and leave
    // projections to normalize in their own context. Nested binders remain intact.
    let first = packet(&10u16, 1u8, [2u16, 3]);
    let second = packet(&20u32, 4u16, [5u32, 6, 7]);
    assert_eq!((first.select)(first.borrowed, &first.values[1]), &3u16);
    assert_eq!((second.select)(second.borrowed, &second.values[2]), &7u32);
    assert_eq!((first.input, second.input), (1u8, 4u16));
    assert_eq!((first.plain, second.plain), (2, 3));
    let Packet { values, borrowed, .. } = first;
    assert_eq!((values, *borrowed), ([2u16, 3], 10u16));
}
