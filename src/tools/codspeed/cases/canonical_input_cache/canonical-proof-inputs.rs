//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver=globally
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

trait Select<'a> {
    fn select(self) -> &'a u8;
}

impl<'a, 'b: 'a> Select<'a> for (&'a u8, &'b u8) {
    fn select(self) -> &'a u8 { self.1 }
}

fn select<'a, T: Select<'a>>(value: T) -> &'a u8 { value.select() }

// Repeated predicates share regions with their ParamEnv. The original variable
// mapping must retain the environment's prefix and each distinct region.
fn independent<'a, 'b, T: 'a + 'b>(
    a: &'a u8,
    b: &'b u8,
    _marker: &T,
) -> (&'a u8, &'b u8)
{
    let first = select((a, a));
    let second = select((b, b));
    (select((first, first)), select((second, second)))
}

fn reversed<'a, 'b, T: 'b + 'a>(
    a: &'a u8,
    b: &'b u8,
    marker: &T,
) -> (&'b u8, &'a u8) {
    independent(b, a, marker)
}

fn shorten<'a, 'b: 'a>(a: &'a u8, b: &'b u8) -> &'a u8 {
    select((a, b))
}

static FIRST: u8 = 7;
static SECOND: u8 = 11;

// Several bodies revisit the same static input and reuse local region numbers
// in different inference contexts, including under parallel compilation.
macro_rules! repeated_body {
    ($($name:ident),*) => { $(
        fn $name() -> (&'static u8, &'static u8) {
            let first = select((&FIRST, &FIRST));
            let second = select((&SECOND, &SECOND));
            (select((first, first)), select((second, second)))
        }
    )* };
}
repeated_body!(repeated_one, repeated_two, repeated_three, repeated_four);

fn apply<F>(f: F, value: &u8) -> &u8
where
    F: for<'a> Fn(&'a u8) -> &'a u8,
{
    f(value)
}

fn borrow(value: &u8) -> &u8 { value }
fn identity<T>(value: T) -> T { value }
fn array<const N: usize>(value: &[u8; N]) -> &[u8; N] { value }

fn main() {
    let outer = 13;
    {
        let inner = 17;
        let (a, b) = independent(&outer, &inner, &());
        assert_eq!((*a, *b), (13, 17));
        let (b, a) = reversed(&outer, &inner, &());
        assert_eq!((*b, *a), (17, 13));
        assert_eq!(*shorten(&inner, &outer), 13);
        assert_eq!(*apply(borrow, &inner), 17);
    }
    for pair in [repeated_one(), repeated_two(), repeated_three(), repeated_four()] {
        assert_eq!((*pair.0, *pair.1), (7, 11));
    }
    let inferred = identity([19, 23]);
    let inferred = array(&inferred);
    assert_eq!(inferred, &[19, 23]);
}
