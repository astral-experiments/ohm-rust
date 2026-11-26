//@ revisions: rpass1 rpass2 rpass3
//@ compile-flags: -Ccodegen-units=16

// Private copies form a cycle through function pointers. Each pending sibling
// still needs placement when the cycle closes, even if a sibling also appears
// in another codegen unit. Edit and restore a link between the copies.
struct Node(fn() -> Node, fn() -> u8);

#[inline(always)]
fn a() -> Node {
    Node(b, first)
}

#[inline(always)]
fn b() -> Node {
    #[cfg(rpass2)]
    let next = a;
    #[cfg(not(rpass2))]
    let next = c;
    Node(next, second)
}

#[inline(always)]
fn c() -> Node {
    Node(a, third)
}

#[inline(always)]
fn first() -> u8 {
    1
}

#[inline(always)]
fn second() -> u8 {
    2
}

#[inline(always)]
fn third() -> u8 {
    3
}

mod one {
    pub static START: fn() -> super::Node = super::a;
}

mod two {
    pub static START: fn() -> super::Node = super::c;
}

fn values(mut next: fn() -> Node) -> [u8; 4] {
    std::array::from_fn(|_| {
        let Node(link, value) = next();
        next = link;
        value()
    })
}

fn main() {
    if cfg!(rpass2) {
        assert_eq!(values(one::START), [1, 2, 1, 2]);
        assert_eq!(values(two::START), [3, 1, 2, 1]);
    } else {
        assert_eq!(values(one::START), [1, 2, 3, 1]);
        assert_eq!(values(two::START), [3, 1, 2, 3]);
    }
}
