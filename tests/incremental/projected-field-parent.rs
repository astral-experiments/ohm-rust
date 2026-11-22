//@ revisions: rpass1 bfail2
//@ compile-flags: -Zincremental-ignore-spans -Zvalidate-mir

struct Tail<T: ?Sized>(T);

enum Entry<'a, 'b> {
    #[cfg(rpass1)]
    Pair(Box<Tail<(&'a str, &'b str)>>),
    #[cfg(bfail2)]
    Pair(Box<Tail<(&'b str, &'a str)>>),
    Empty,
}

// The final field carries a type, but we still need to relate it to
// the field of its parent at every level of the projection.
fn first<'a, 'b>(entries: &[Entry<'a, 'b>]) -> &'a str {
    match &entries[0] {
        Entry::Pair(pair) => pair.0.0,
        //[bfail2]~^ ERROR lifetime may not live long enough
        Entry::Empty => "",
    }
}

fn main() {
    let tail: &Tail<[u8]> = &Tail([7, 8]);
    assert_eq!(tail.0[1], 8);
    let entries = [Entry::Pair(Box::new(Tail(("a", "b")))), Entry::Empty];
    assert_eq!(first(&entries), "a");
}
