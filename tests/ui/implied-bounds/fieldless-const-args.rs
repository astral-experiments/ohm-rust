// A fieldless struct with concrete const arguments cannot imply outlives
// bounds, but its explicit where-clauses still need to be checked.

pub struct BitsImpl<const N: usize>;

pub trait Bits {
    const VALUE: usize;
}

impl Bits for BitsImpl<7> {
    const VALUE: usize = 7;
}

impl BitsImpl<7> {
    fn new() -> Self {
        BitsImpl
    }
}

fn round_trip(bits: BitsImpl<7>) -> BitsImpl<7> {
    bits
}

trait Allowed {}
impl Allowed for [u8; 7] {}

struct Gate<const N: usize>
where
    [u8; N]: Allowed;

fn allowed(_: Gate<7>) {}

fn disallowed(_: Gate<8>) {}
//~^ ERROR the trait bound `[u8; 8]: Allowed` is not satisfied

fn main() {
    let _ = round_trip(BitsImpl::<7>::new());
}
